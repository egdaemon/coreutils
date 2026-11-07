// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Runs utilities as processes for the guest modules of eg, which call `execveatinit` to start a
//! utility and poll `execveatwait` until it exits.
//!
//! A process is described by the guest in `{runtime}/proc/{name}`, after procfs:
//! - `cmdline`: the argv, NUL separated.
//! - `environ`: the environment, NUL separated `KEY=VALUE` entries.
//! - `cwd`: the path of the working directory. A file rather than a link like procfs, wasi does
//!   not permit links to absolute paths.
//! - `fd/0`, `fd/1`, `fd/2`: unix sockets the reactor listens on for the utility's stdio, the
//!   guest connects to them once `execveatinit` returns.
//!
//! `name` is the id of the process, as unpadded url safe base64 of its big endian bytes. A wasm
//! module is a single thread: neither call blocks. `execveatwait` advances the utility as far as
//! its stdio allows and returns [`ERR_PENDING`] until it exits, so the guest runs between polls.

use super::process::{Environment, Stream, Utility, lookup, run};
use super::wasinet::{Fill, Socket};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, Write};
use std::os::wasi::ffi::OsStringExt;
use std::path::{Path, PathBuf};

// the codes of eg's ffierrors, beyond the range of exit codes.
const ERR_UNRECOVERABLE: u32 = 1000;
const ERR_NOT_FOUND: u32 = 1001;
const ERR_PENDING: u32 = 1002;

// bounds the work of a single execveatwait, so the guest runs regularly.
const BUDGET: usize = 256 * 1024;
// bounds a single read of stdin, so a guest writing quickly can not starve the others.
const FILL: usize = 16 * 1024;

struct Proc {
    utility: Utility,
    argv: Vec<OsString>,
    // the started stream, of a utility that streams.
    stream: Option<Box<dyn Stream>>,
    env: Environment,
    // kept open so the guest's connections are accepted.
    listeners: [Socket; 3],
    conns: [Option<Socket>; 3],
    // stdin read so far, the utility runs once it is exhausted.
    stdin: Vec<u8>,
    eof: bool,
    // stdout and stderr, with the offset written so far.
    out: [(Vec<u8>, usize); 2],
    code: Option<i32>,
}

thread_local! {
    static PROCS: RefCell<HashMap<u64, Proc>> = RefCell::new(HashMap::new());
}

fn runtime() -> PathBuf {
    std::env::var_os("EG_COMPUTE_RUNTIME_DIRECTORY")
        .map_or_else(|| PathBuf::from("/eg.mnt/.eg.runtime"), PathBuf::from)
}

/// The name of the process' directory: unpadded url safe base64 of the id's big endian bytes.
fn name(id: u64) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bytes = id.to_be_bytes();
    let mut name = String::with_capacity(11);
    for chunk in bytes.chunks(3) {
        let mut b = [0u8; 3];
        b[..chunk.len()].copy_from_slice(chunk);
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        for i in 0..=chunk.len() {
            name.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    name
}

fn split(raw: Vec<u8>) -> Vec<OsString> {
    raw.split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| OsString::from_vec(s.to_vec()))
        .collect()
}

/// Starts the utility of the process `id`. Returns 0, or [`ERR_NOT_FOUND`] when the utility is not
/// available as a process.
#[unsafe(export_name = "execveatinit")]
pub extern "C" fn execveatinit(id: u64) -> u32 {
    match init(id) {
        Ok(Some(proc)) => {
            PROCS.with_borrow_mut(|procs| procs.insert(id, proc));
            0
        }
        Ok(None) => ERR_NOT_FOUND,
        Err(err) => {
            eprintln!("execveatinit: {err}");
            ERR_UNRECOVERABLE
        }
    }
}

/// Adds the path an operation failed on to its error.
fn at<T>(path: &Path, r: io::Result<T>) -> io::Result<T> {
    r.map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))
}

fn init(id: u64) -> io::Result<Option<Proc>> {
    let dir = runtime().join("proc").join(name(id));
    let read = |name: &str| {
        let path = dir.join(name);
        at(&path, std::fs::read(&path))
    };
    let listen = |fd: &str| {
        let path = dir.join("fd").join(fd);
        at(&path, Socket::listen(&path))
    };
    let argv = split(read("cmdline")?);
    let Some(argv0) = argv.first().cloned() else {
        return Ok(None);
    };
    let util = Path::new(&argv0)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(utility) = lookup(&util) else {
        return Ok(None);
    };

    let vars = split(read("environ")?)
        .into_iter()
        .filter_map(|kv| {
            let kv = kv.into_vec();
            let eq = kv.iter().position(|b| *b == b'=')?;
            Some((
                OsString::from_vec(kv[..eq].to_vec()),
                OsString::from_vec(kv[eq + 1..].to_vec()),
            ))
        })
        .collect();
    let env = Environment {
        util,
        argv0,
        vars,
        cwd: PathBuf::from(OsString::from_vec(read("cwd")?)),
    };

    let listeners = [listen("0")?, listen("1")?, listen("2")?];

    Ok(Some(Proc {
        utility,
        argv,
        stream: None,
        env,
        listeners,
        conns: [None, None, None],
        stdin: Vec::new(),
        eof: false,
        out: [(Vec::new(), 0), (Vec::new(), 0)],
        code: None,
    }))
}

/// Polls the process `id`. Returns [`ERR_PENDING`] until the utility exits, then its exit code.
/// The process is gone once its exit code is returned.
#[unsafe(export_name = "execveatwait")]
pub extern "C" fn execveatwait(id: u64) -> u32 {
    let Some(mut proc) = PROCS.with_borrow_mut(|procs| procs.remove(&id)) else {
        return ERR_NOT_FOUND;
    };

    match advance(&mut proc) {
        Ok(Some(code)) => code as u32,
        Ok(None) => {
            PROCS.with_borrow_mut(|procs| procs.insert(id, proc));
            ERR_PENDING
        }
        Err(err) => {
            eprintln!("execveatwait: {err}");
            ERR_UNRECOVERABLE
        }
    }
}

/// Advances the process as far as its stdio allows, within the budget. Returns the exit code once
/// the utility exited and its output was written.
///
/// A utility that streams consumes each chunk of stdin as it arrives. Until utilities can suspend,
/// the others run once their stdin is exhausted, against the stdin read so far, so they never
/// wait on the guest. Output is buffered and written between steps.
fn advance(proc: &mut Proc) -> io::Result<Option<i32>> {
    let mut moved = 0;

    for (listener, conn) in proc.listeners.iter().zip(proc.conns.iter_mut()) {
        if conn.is_none() {
            match listener.accept() {
                Ok(c) => *conn = Some(c),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }
    let [Some(stdin), Some(stdout), Some(stderr)] = &mut proc.conns else {
        unreachable!("every connection was accepted");
    };

    while moved < BUDGET {
        // output is written before more input is consumed, so a reader that falls behind slows
        // the utility down instead of growing the buffers.
        for ((out, offset), conn) in proc.out.iter_mut().zip([&mut *stdout, &mut *stderr]) {
            while *offset < out.len() {
                match conn.write(&out[*offset..]) {
                    // the guest's reader is gone, nothing more will be written.
                    Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                    Ok(n) => {
                        *offset += n;
                        moved += n;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                    Err(e) => return Err(e),
                }
            }
            out.clear();
            *offset = 0;
        }

        if let Some(code) = proc.code {
            return Ok(Some(code));
        }

        let [(stdout, _), (stderr, _)] = &mut proc.out;
        if let (Utility::Stream(spawn), None) = (proc.utility, &proc.stream) {
            let _restore = proc.env.enter()?;
            match spawn(std::mem::take(&mut proc.argv), stderr) {
                Ok(s) => proc.stream = Some(s),
                Err(code) => proc.code = Some(code),
            }
            continue;
        }

        if !proc.eof {
            match stdin.fill(&mut proc.stdin, FILL)? {
                Fill::Data(n) => moved += n,
                Fill::Pending => return Ok(None),
                Fill::Eof => proc.eof = true,
            }
            // a stream consumes each chunk as it arrives.
            if let Some(s) = proc.stream.as_mut()
                && !proc.stdin.is_empty()
            {
                let _restore = proc.env.enter()?;
                if let Err(code) = s.write(&proc.stdin, stdout) {
                    proc.code = Some(code);
                }
                proc.stdin.clear();
            }
            continue;
        }

        let _restore = proc.env.enter()?;
        proc.code = Some(match (proc.utility, proc.stream.take()) {
            (_, Some(s)) => s.finish(),
            (Utility::Main(main), None) => {
                let argv = std::mem::take(&mut proc.argv);
                let input = std::mem::take(&mut proc.stdin);
                run(main, argv, &mut input.as_slice(), stdout, stderr)
            }
            (Utility::Stream(_), None) => unreachable!("a stream is started before stdin is read"),
        });
    }

    Ok(None)
}
