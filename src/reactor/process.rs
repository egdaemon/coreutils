// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Utilities the reactor runs as processes (see `execveat`).
//!
//! A process advances a step at a time: each step consumes a chunk of stdin and writes what it
//! produced to the stdout/stderr buffers, so the reactor never blocks on the utility and can keep
//! several in flight. A process only exists for utilities with an entry point that can be driven
//! this way, `uumain` runs to completion on the standard streams.

use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use uucore::error::UError;

pub trait Process {
    /// Whether the utility reads stdin, the reactor only reads it for those that do.
    fn stdin(&self) -> bool;

    /// Consumes a chunk of stdin, `None` once it is exhausted or for utilities without stdin.
    /// Returns the exit code once the utility is done.
    fn step(
        &mut self,
        input: Option<&[u8]>,
        stdout: &mut Vec<u8>,
        stderr: &mut Vec<u8>,
    ) -> Option<i32>;
}

/// Starts the utility from its argv, a utility that fails to start returns its exit code.
pub type Spawn = fn(Vec<OsString>, &mut Vec<u8>) -> Result<Box<dyn Process>, i32>;

/// The utilities available as processes.
pub fn lookup(util: &str) -> Option<Spawn> {
    match util {
        #[cfg(feature = "echo")]
        "echo" => Some(spawn_echo),
        #[cfg(feature = "tee")]
        "tee" => Some(spawn_tee),
        _ => None,
    }
}

/// Reports an error the way `uucore::main` does, returning its exit code.
fn report(err: &dyn UError, stderr: &mut Vec<u8>) -> i32 {
    let msg = err.to_string();
    if !msg.is_empty() {
        let _ = writeln!(stderr, "{}: {msg}", uucore::util_name());
    }
    if err.usage() {
        let _ = writeln!(
            stderr,
            "Try '{} --help' for more information.",
            uucore::execution_phrase()
        );
    }
    err.code()
}

#[cfg(feature = "echo")]
struct Echo(Vec<OsString>);

#[cfg(feature = "echo")]
fn spawn_echo(argv: Vec<OsString>, _: &mut Vec<u8>) -> Result<Box<dyn Process>, i32> {
    Ok(Box::new(Echo(argv)))
}

#[cfg(feature = "echo")]
impl Process for Echo {
    fn stdin(&self) -> bool {
        false
    }

    // the output is bounded by argv, so echo completes in its first step.
    fn step(
        &mut self,
        _: Option<&[u8]>,
        stdout: &mut Vec<u8>,
        stderr: &mut Vec<u8>,
    ) -> Option<i32> {
        let argv = std::mem::take(&mut self.0);
        Some(match ::echo::uumain_with(argv.into_iter(), stdout) {
            Ok(()) => uucore::error::get_exit_code(),
            Err(err) => report(err.as_ref(), stderr),
        })
    }
}

#[cfg(feature = "tee")]
struct Tee(Option<::tee::Tee>);

#[cfg(feature = "tee")]
fn spawn_tee(argv: Vec<OsString>, stderr: &mut Vec<u8>) -> Result<Box<dyn Process>, i32> {
    match ::tee::Tee::new(argv.into_iter()) {
        Ok(t) => Ok(Box::new(Tee(Some(t)))),
        Err(err) => Err(report(err.as_ref(), stderr)),
    }
}

#[cfg(feature = "tee")]
impl Process for Tee {
    fn stdin(&self) -> bool {
        true
    }

    fn step(&mut self, input: Option<&[u8]>, stdout: &mut Vec<u8>, _: &mut Vec<u8>) -> Option<i32> {
        let t = self.0.as_mut()?;
        match input {
            Some(buf) => t.write(buf, stdout).err().map(|()| 1),
            None => self.0.take().map(::tee::Tee::finish),
        }
    }
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
/// The environment and working directory of a command. The reactor is shared by every command,
/// so a command's environment is only applied while the reactor calls into its process.
pub struct Environment {
    pub util: String,
    pub argv0: OsString,
    pub vars: Vec<(OsString, OsString)>,
    pub cwd: PathBuf,
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
impl Environment {
    /// Applies the environment until the returned guard drops, which restores the previous one.
    pub fn enter(&self) -> io::Result<Restore> {
        let restore = Restore {
            vars: env::vars_os().collect(),
            cwd: env::current_dir()?,
        };
        replace(&self.vars);
        chdir(&self.cwd)?;
        super::invocation(&self.util, &self.argv0).map_err(io::Error::other)?;
        Ok(restore)
    }
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
pub struct Restore {
    vars: Vec<(OsString, OsString)>,
    cwd: PathBuf,
}

impl Drop for Restore {
    fn drop(&mut self) {
        replace(&self.vars);
        let _ = chdir(&self.cwd);
    }
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
fn replace(vars: &[(OsString, OsString)]) {
    // SAFETY: the reactor is single threaded (wasm32-wasip1), nothing reads the environment
    // concurrently.
    unsafe {
        for (k, _) in env::vars_os() {
            env::remove_var(k);
        }
        for (k, v) in vars {
            env::set_var(k, v);
        }
    }
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
/// chdir through libc, which tracks the working directory of a wasi module.
fn chdir(dir: &std::path::Path) -> io::Result<()> {
    let dir = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes())?;
    // SAFETY: dir is a NUL terminated string that outlives the call.
    if unsafe { libc::chdir(dir.as_ptr()) } == 0 {
        return Ok(());
    }
    Err(io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn run(p: &mut dyn Process, input: &[&[u8]]) -> (i32, Vec<u8>, Vec<u8>) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        for chunk in input {
            if let Some(code) = p.step(Some(chunk), &mut stdout, &mut stderr) {
                return (code, stdout, stderr);
            }
        }
        let code = p.step(None, &mut stdout, &mut stderr).expect("done at eof");
        (code, stdout, stderr)
    }

    #[test]
    fn unknown_utility_is_not_a_process() {
        assert!(lookup("ls").is_none());
    }

    #[cfg(feature = "echo")]
    #[test]
    fn echo_writes_its_arguments() {
        let mut p =
            lookup("echo").unwrap()(argv(&["echo", "-n", "a", "b"]), &mut Vec::new()).unwrap();
        assert!(!p.stdin());
        assert_eq!(run(p.as_mut(), &[]), (0, b"a b".to_vec(), Vec::new()));
    }

    #[cfg(feature = "echo")]
    #[test]
    fn echo_interprets_escapes() {
        let mut p =
            lookup("echo").unwrap()(argv(&["echo", "-e", r"a\tb"]), &mut Vec::new()).unwrap();
        assert_eq!(run(p.as_mut(), &[]), (0, b"a\tb\n".to_vec(), Vec::new()));
    }

    #[cfg(feature = "tee")]
    #[test]
    fn tee_copies_each_chunk_to_stdout_and_files() {
        let dir = env::temp_dir().join(format!("reactor-tee-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("out");

        let mut p = lookup("tee").unwrap()(
            vec!["tee".into(), file.clone().into_os_string()],
            &mut Vec::new(),
        )
        .unwrap();
        assert!(p.stdin());

        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        assert_eq!(p.step(Some(b"ab"), &mut stdout, &mut stderr), None);
        // the output of a chunk is available before the input is exhausted.
        assert_eq!(stdout, b"ab");
        assert_eq!(p.step(Some(b"c"), &mut stdout, &mut stderr), None);
        assert_eq!(p.step(None, &mut stdout, &mut stderr), Some(0));
        assert_eq!(stdout, b"abc");
        assert_eq!(std::fs::read(&file).unwrap(), b"abc");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "tee")]
    #[test]
    fn tee_rejects_unknown_options() {
        let mut stderr = Vec::new();
        let code = match lookup("tee").unwrap()(argv(&["tee", "--bogus"]), &mut stderr) {
            Ok(_) => panic!("expected tee to fail to start"),
            Err(code) => code,
        };
        assert_eq!(code, 1);
    }
}
