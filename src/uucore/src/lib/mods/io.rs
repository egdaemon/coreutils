// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Encapsulates differences between OSs regarding the access to
//! file handles / descriptors.
//! This is useful when dealing with lower level stdin/stdout access.
//!
//! In detail:
//! On unix like OSs, file _descriptors_ are used in this context.
//! On windows OSs, file _handles_ are used.
//!
//! Even though they are distinct classes, they share common functionality.
//! Access to this common functionality is provided in `OwnedFileDescriptorOrHandle`.

use std::fs::{File, OpenOptions};
use std::io;
#[cfg(not(windows))]
use std::os::fd::{AsFd, OwnedFd};
#[cfg(windows)]
use std::os::windows::io::{AsHandle, OwnedHandle};
use std::path::Path;
use std::process::Stdio;

#[cfg(windows)]
type NativeType = OwnedHandle;
#[cfg(not(windows))]
type NativeType = OwnedFd;

// create reader without buffering
#[cfg(any(unix, target_os = "wasi"))]
pub struct RawReader<T: AsFd>(pub T);
#[cfg(any(unix, target_os = "wasi"))]
impl<T: AsFd> io::Read for RawReader<T> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        rustix::io::read(&self.0, b).map_err(Into::into)
    }
}

// create writer without buffering
#[cfg(any(unix, target_os = "wasi"))]
pub struct RawWriter<T: AsFd>(pub T);
#[cfg(any(unix, target_os = "wasi"))]
impl<T: AsFd> io::Write for RawWriter<T> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        rustix::io::write(&self.0, b).map_err(Into::into)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// abstraction wrapper for native file handle / file descriptor
// todo: remove clone introducing additional syscall dependency
pub struct OwnedFileDescriptorOrHandle {
    fx: NativeType,
}

impl OwnedFileDescriptorOrHandle {
    /// create from underlying native type
    pub fn new(x: NativeType) -> Self {
        Self { fx: x }
    }

    /// create by opening a file
    pub fn open_file(options: &OpenOptions, path: &Path) -> io::Result<Self> {
        let f = options.open(path)?;
        Self::from(f)
    }

    /// conversion from borrowed native type
    ///
    /// e.g. `std::io::stdout()`, `std::fs::File`, ...
    #[cfg(windows)]
    pub fn from<T: AsHandle>(t: T) -> io::Result<Self> {
        Ok(Self {
            fx: t.as_handle().try_clone_to_owned()?,
        })
    }

    /// conversion from borrowed native type
    ///
    /// e.g. `std::io::stdout()`, `std::fs::File`, ...
    #[cfg(not(windows))]
    pub fn from<T: AsFd>(t: T) -> io::Result<Self> {
        Ok(Self {
            fx: t.as_fd().try_clone_to_owned()?,
        })
    }

    /// instantiates a corresponding `File`
    pub fn into_file(self) -> File {
        File::from(self.fx)
    }

    /// instantiates a corresponding `Stdio`
    #[cfg(not(target_os = "wasi"))]
    pub fn into_stdio(self) -> Stdio {
        Stdio::from(self.fx)
    }

    /// WASI: `Stdio::from(OwnedFd)` is not available, convert via File instead.
    #[cfg(target_os = "wasi")]
    pub fn into_stdio(self) -> Stdio {
        Stdio::from(File::from(self.fx))
    }

    /// clones self. useful when needing another
    /// owned reference to same file
    pub fn try_clone(&self) -> io::Result<Self> {
        self.fx.try_clone().map(Self::new)
    }

    /// provides native type to be used with
    /// OS specific functions without abstraction
    pub fn as_raw(&self) -> &NativeType {
        &self.fx
    }
}

/// instantiates a corresponding `Stdio`
impl From<OwnedFileDescriptorOrHandle> for Stdio {
    fn from(value: OwnedFileDescriptorOrHandle) -> Self {
        value.into_stdio()
    }
}

/// The standard streams of a utility run in isolation, in place of the process's own.
struct Isolated {
    stdin: io::BufReader<Box<dyn io::Read>>,
    stdout: Vec<u8>,
}

thread_local! {
    static ISOLATED: std::cell::RefCell<Option<Isolated>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` with `stdin` in place of the process's stdin for [`crate::read_yes`] and
/// [`stdin_is_terminal`], then writes what `f` wrote to [`stdout`] to `stdout`. For utilities whose
/// prompts and messages are spread too deep to thread their streams through.
pub fn isolate<T>(
    stdin: Box<dyn io::Read>,
    stdout: &mut dyn io::Write,
    f: impl FnOnce() -> T,
) -> io::Result<T> {
    ISOLATED.with_borrow_mut(|i| {
        *i = Some(Isolated {
            stdin: io::BufReader::new(stdin),
            stdout: Vec::new(),
        });
    });
    let result = f();
    let out = ISOLATED
        .with_borrow_mut(Option::take)
        .map(|i| i.stdout)
        .unwrap_or_default();
    stdout.write_all(&out)?;
    Ok(result)
}

/// Reads a line of the isolated stdin, `None` when the utility is not isolated.
pub(crate) fn read_line_isolated(s: &mut String) -> Option<io::Result<usize>> {
    use io::BufRead;
    ISOLATED.with_borrow_mut(|i| i.as_mut().map(|i| i.stdin.read_line(s)))
}

/// Whether stdin is a terminal. The stdin of an isolated utility never is.
pub fn stdin_is_terminal() -> bool {
    use io::IsTerminal;
    ISOLATED.with_borrow(Option::is_none) && io::stdin().is_terminal()
}

/// The process's stdout, or the stdout of an isolated utility (see [`isolate`]).
pub struct Stdout;

/// Returns [`Stdout`].
pub fn stdout() -> Stdout {
    Stdout
}

/// Writes a line to [`stdout`], see [`crate::outln`].
pub fn outln(args: std::fmt::Arguments) {
    use io::Write;
    writeln!(stdout(), "{args}").expect("failed printing to stdout");
}

impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match ISOLATED.with_borrow_mut(|i| i.as_mut().map(|i| io::Write::write(&mut i.stdout, buf)))
        {
            Some(n) => n,
            None => io::Write::write(&mut io::stdout(), buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if ISOLATED.with_borrow(Option::is_some) {
            return Ok(());
        }
        io::Write::flush(&mut io::stdout())
    }
}

/// Read and discard up to `n` bytes from `reader`, using a `buf_size` buffer.
///
/// Returns the number of bytes actually read; a value less than `n` means the
/// reader hit EOF first. Reads are retried on [`io::ErrorKind::Interrupted`].
/// This is used to skip over the start of an input that cannot be `seek`ed.
pub fn read_and_discard<R: io::Read>(reader: &mut R, n: u64, buf_size: usize) -> io::Result<u64> {
    use io::Read;
    let mut buf = Vec::new();
    buf.try_reserve(buf_size.min(n as usize))?;
    let mut total = 0u64;
    while total < n {
        let to_read = (n - total).min(buf_size as u64);
        buf.clear();
        match reader.by_ref().take(to_read).read_to_end(&mut buf) {
            Ok(0) => break, // EOF
            Ok(read) => total += read as u64,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::read_and_discard;
    use std::io::Cursor;

    #[test]
    fn discard_within_input() {
        let mut r = Cursor::new(b"abcdefgh".to_vec());
        assert_eq!(read_and_discard(&mut r, 3, 4).unwrap(), 3);
        assert_eq!(r.position(), 3);
    }

    #[test]
    fn discard_stops_at_eof() {
        let mut r = Cursor::new(b"abc".to_vec());
        // Asking for more than is available returns only what was read.
        assert_eq!(read_and_discard(&mut r, 100, 4).unwrap(), 3);
    }
}
