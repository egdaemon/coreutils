// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Unix sockets through the host's `wasinet_v0` module.
//!
//! The descriptors the host returns are native to the host and not wasi descriptors, so they are
//! only usable through these imports: `fd_read`/`fd_write` do not reach them. The host opens
//! every socket non-blocking, an operation that would block fails with
//! [`io::ErrorKind::WouldBlock`].

use std::io::{self, Read, Write};
use std::os::wasi::ffi::OsStrExt;
use std::path::Path;

// wasi errno for EAGAIN.
const EAGAIN: i32 = 6;
// wasi numbering, the host translates them.
const AF_UNIX: i32 = 1;
const SOCK_STREAM: i32 = 1;
const SHUT_RDWR: i32 = 3;

// the layout of wasinet's Vector: the host reads it as a Go struct of a 64 bit pointer and a
// 32 bit length, padded to 16 bytes.
#[repr(C)]
struct Iovec {
    ptr: u64,
    len: u32,
    _pad: u32,
}

impl Iovec {
    fn new(ptr: *const u8, len: usize) -> Self {
        Self {
            ptr: ptr as usize as u64,
            len: len as u32,
            _pad: 0,
        }
    }
}

// the layout of wasinet's RawSocketAddress for the unix family: the family and type are repeated
// ahead of the path, as the guest copies its whole address after them.
#[repr(C)]
struct SockAddr {
    family: u16,
    soctype: u16,
    _family: u16,
    _soctype: u16,
    path: [u8; 122],
}

#[link(wasm_import_module = "wasinet_v0")]
unsafe extern "C" {
    fn sock_open(af: i32, socktype: i32, proto: i32, fd: *mut i32) -> i32;
    fn sock_bind(fd: i32, addr: *const SockAddr, addrlen: u32) -> i32;
    fn sock_listen(fd: i32, backlog: i32) -> i32;
    fn sock_accept(fd: i32, nfd: *mut i32, addr: *mut SockAddr, addrlen: u32) -> i32;
    #[allow(clippy::too_many_arguments)]
    fn sock_recv_from(
        fd: i32,
        iovs: *const Iovec,
        iovslen: u32,
        oob: *mut u8,
        ooblen: u32,
        addr: *mut SockAddr,
        addrlen: u32,
        iflags: i32,
        nread: *mut u32,
        oflags: *mut u32,
    ) -> i32;
    #[allow(clippy::too_many_arguments)]
    fn sock_send_to(
        fd: i32,
        iovs: *const Iovec,
        iovslen: u32,
        oob: *const u8,
        ooblen: u32,
        addr: *const SockAddr,
        addrlen: u32,
        flags: i32,
        nwritten: *mut u32,
    ) -> i32;
    fn sock_shutdown(fd: i32, how: i32) -> i32;
    fn sock_determine_host_af_family(af: i32) -> i32;
}

fn errno(code: i32) -> io::Result<()> {
    match code {
        0 => Ok(()),
        EAGAIN => Err(io::ErrorKind::WouldBlock.into()),
        code => Err(io::Error::from_raw_os_error(code)),
    }
}

/// An address of the unix family, the host requires one even to send on a connected socket.
fn unix(path: &[u8]) -> io::Result<SockAddr> {
    // SAFETY: a pure query of the host.
    let family = unsafe { sock_determine_host_af_family(AF_UNIX) } as u16;
    let mut addr = SockAddr {
        family,
        soctype: SOCK_STREAM as u16,
        _family: family,
        _soctype: SOCK_STREAM as u16,
        path: [0; 122],
    };
    // the path is NUL terminated within the address.
    if path.len() >= addr.path.len() {
        return Err(io::ErrorKind::InvalidFilename.into());
    }
    addr.path[..path.len()].copy_from_slice(path);
    Ok(addr)
}

/// A unix stream socket of the host, closed on drop.
pub struct Socket(i32);

impl Socket {
    /// Listens on `path` for a single connection.
    pub fn listen(path: &Path) -> io::Result<Self> {
        let addr = unix(path.as_os_str().as_bytes())?;
        let mut fd = -1;
        // SAFETY: fd outlives the call.
        errno(unsafe { sock_open(addr.family.into(), SOCK_STREAM, 0, &raw mut fd) })?;
        let socket = Self(fd);
        // SAFETY: addr outlives the call.
        errno(unsafe { sock_bind(fd, &raw const addr, size_of::<SockAddr>() as u32) })?;
        // SAFETY: plain values.
        errno(unsafe { sock_listen(fd, 1) })?;
        Ok(socket)
    }

    /// Accepts a pending connection.
    pub fn accept(&self) -> io::Result<Self> {
        let mut addr = unix(b"")?;
        let mut fd = -1;
        // SAFETY: fd and addr outlive the call.
        errno(unsafe {
            sock_accept(
                self.0,
                &raw mut fd,
                &raw mut addr,
                size_of::<SockAddr>() as u32,
            )
        })?;
        Ok(Self(fd))
    }
}

impl Read for Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let iov = Iovec::new(buf.as_mut_ptr(), buf.len());
        let mut addr = unix(b"")?;
        let (mut n, mut oflags) = (0u32, 0u32);
        // SAFETY: buf, addr and the results outlive the call.
        errno(unsafe {
            sock_recv_from(
                self.0,
                &raw const iov,
                1,
                std::ptr::null_mut(),
                0,
                &raw mut addr,
                size_of::<SockAddr>() as u32,
                0,
                &raw mut n,
                &raw mut oflags,
            )
        })?;
        Ok(n as usize)
    }
}

impl Write for Socket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let iov = Iovec::new(buf.as_ptr(), buf.len());
        let addr = unix(b"")?;
        let mut n = 0u32;
        // SAFETY: buf, addr and the result outlive the call.
        errno(unsafe {
            sock_send_to(
                self.0,
                &raw const iov,
                1,
                std::ptr::null(),
                0,
                &raw const addr,
                size_of::<SockAddr>() as u32,
                0,
                &raw mut n,
            )
        })?;
        Ok(n as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        // SAFETY: plain values, shutting down both directions closes the socket.
        unsafe { sock_shutdown(self.0, SHUT_RDWR) };
    }
}
