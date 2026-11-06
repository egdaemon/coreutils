// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Reactor-style bindings for hosts that call utilities directly instead of running `_start`.
//!
//! Every enabled utility is exported as `uu_<name>(args: *const u8, len: usize) -> i32`
//! (see `build.rs`). `args` points at `len` bytes holding the argv of the utility, each entry
//! (including argv[0]) terminated by a NUL byte. The return value is the utility's exit code.
//!
//! The host passes argv through guest memory it obtains from [`uu_alloc`] and releases with
//! [`uu_free`]. Utilities that call `process::exit` terminate the instance, as they do in the
//! command form.
//!
//! The module may be kept alive across many calls, but a call must finish before the next one
//! starts (the module is single threaded). Per call state (argv[0], the exit code) is reset on
//! entry. Other process wide state kept by utilities is not reset.

use std::ffi::OsString;
use std::io::Write;
use uucore::locale;

/// Allocate `len` bytes of guest memory for the host to write arguments into.
#[unsafe(export_name = "uu_alloc")]
pub extern "C" fn uu_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Release memory obtained from [`uu_alloc`], passing the same `len`.
///
/// # Safety
/// `ptr` must come from `uu_alloc(len)` and must not be used afterwards.
#[unsafe(export_name = "uu_free")]
pub unsafe extern "C" fn uu_free(ptr: *mut u8, len: usize) {
    // SAFETY: guaranteed by the caller, mirrors the allocation in `uu_alloc`.
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len.max(1)) });
}

/// Change the working directory that the following calls resolve relative paths against, since the module is
/// shared by calls made from different directories. Returns 0, or the errno of the failure.
///
/// # Safety
/// `path` must point at `len` readable bytes.
#[unsafe(export_name = "uu_chdir")]
pub unsafe extern "C" fn uu_chdir(path: *const u8, len: usize) -> i32 {
    // SAFETY: guaranteed by the caller.
    let raw = unsafe { std::slice::from_raw_parts(path, len) };
    let Ok(dir) = std::ffi::CString::new(raw) else {
        return libc::EINVAL;
    };
    // SAFETY: dir is a NUL terminated string that outlives the call.
    if unsafe { libc::chdir(dir.as_ptr()) } == 0 {
        return 0;
    }
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

fn run(
    util: &str,
    uumain: fn(std::vec::IntoIter<OsString>) -> i32,
    args: *const u8,
    len: usize,
) -> i32 {
    let raw = if len == 0 {
        &[][..]
    } else {
        // SAFETY: the host promises `args` points at `len` readable bytes.
        unsafe { std::slice::from_raw_parts(args, len) }
    };

    let mut argv = Vec::new();
    for arg in raw.split_inclusive(|b| *b == 0) {
        let Some(arg) = arg.strip_suffix(&[0]) else {
            eprintln!("{util}: argument is not NUL terminated");
            return 2;
        };
        let Ok(arg) = std::str::from_utf8(arg) else {
            eprintln!("{util}: argument is not valid UTF-8");
            return 2;
        };
        argv.push(OsString::from(arg));
    }
    if argv.is_empty() {
        argv.push(OsString::from(util));
    }

    // The module is long lived and shared by every call, so nothing may carry over between them:
    // argv[0] is per call and the exit code of a previous call must not leak into this one.
    let argv0 = argv[0].to_string_lossy().into_owned();
    let name = std::path::Path::new(&argv0)
        .file_name()
        .map_or_else(|| argv0.clone(), |n| n.to_string_lossy().into_owned());
    uucore::set_invocation(&name, &argv0);
    uucore::error::set_exit_code(0);

    let canonical = match util {
        "dir" | "vdir" => "ls",
        other => other,
    };
    if let Err(err) = locale::setup_localization(canonical) {
        eprintln!("Could not init the localization system: {err}");
        return 99;
    }

    let code = uumain(argv.into_iter());
    let _ = std::io::stdout().flush();
    code
}

include!(concat!(env!("OUT_DIR"), "/uutils_reactor.rs"));
