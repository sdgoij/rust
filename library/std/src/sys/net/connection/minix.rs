//! Socket API over the Minix `/dev/tcp` and `/dev/udp` clone-minor devices.
//!
//! The socket operations (open, bind/connect/listen/accept, send/recv,
//! shutdown, peer/local addresses) delegate to `minix_std::net`, which
//! encodes the reference `NWIO*` ioctls and handles the UDP datagram
//! header protocol. This module only adapts the std-facing
//! `TcpStream`/`TcpListener`/`UdpSocket` types to the std API.

use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, SocketAddrV4, ToSocketAddrs};
use crate::os::fd::FromRawFd;
use crate::sys::fd::FileDesc;
use crate::sys::net::connection::each_addr;
use crate::sys::pal::minix::fs::{read, write};
use crate::sys::pal::minix::syscall;
use crate::sys::unsupported;
use crate::time::Duration;

fn minix_err(e: minix_std::MinixErr) -> io::Error {
    io::Error::from_raw_os_error(e.0)
}

/// Convert a std address to the IPv4 octets Minix uses. IPv6 is not
/// supported by the net server.
fn to_ip(addr: &SocketAddr) -> io::Result<[u8; 4]> {
    match addr {
        SocketAddr::V4(v4) => Ok(v4.ip().octets()),
        SocketAddr::V6(_) => {
            Err(io::const_error!(io::ErrorKind::Unsupported, "IPv6 is not supported"))
        }
    }
}

fn from_ip_port(ip: [u8; 4], port: u16) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(ip), port))
}

fn to_sockaddr(a: minix_std::net::SocketAddr) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(a.ip), a.port))
}

/// Open a `/dev/tcp` socket descriptor. The local port is auto-assigned at
/// connect/bind time.
fn tcp_socket_fd() -> io::Result<i32> {
    minix_std::net::tcp_socket().map_err(minix_err)
}

/// Open a `/dev/udp` socket descriptor and implicitly bind it to an
/// ephemeral local port on the local address.
fn udp_socket_fd() -> io::Result<i32> {
    minix_std::net::udp_socket().map_err(minix_err)
}

fn peer_addr(fd: i32) -> io::Result<SocketAddr> {
    let (ip, port) = minix_std::net::getpeername(fd).map_err(minix_err)?;
    Ok(from_ip_port(ip, port))
}

fn socket_addr(fd: i32) -> io::Result<SocketAddr> {
    let (ip, port) = minix_std::net::getsockname(fd).map_err(minix_err)?;
    Ok(from_ip_port(ip, port))
}

/// Duplicate a descriptor (`F_DUPFD`).
fn duplicate(fd: i32) -> io::Result<FileDesc> {
    let ret = syscall::fcntl(fd, syscall::F_DUPFD, 3);
    if ret < 0 {
        Err(io::Error::from_raw_os_error(-ret as i32))
    } else {
        Ok(unsafe { FileDesc::from_raw_fd(ret as i32) })
    }
}

/// Accept the next pending connection (`minix_std::net::accept` retries the
/// EAGAIN the net server returns when its bounded accept poll expires).
fn accept(fd: i32) -> io::Result<FileDesc> {
    let newfd = minix_std::net::accept(fd).map_err(minix_err)?;
    Ok(unsafe { FileDesc::from_raw_fd(newfd) })
}

/// Bound retries when the net server's bounded RX poll reports "no data"
/// (it returns 0 both for that and for a peer FIN, so the no-data case is
/// retried before EOF is reported).
const READ_RETRIES: u32 = 4;

/// Short-write retries before reporting a partial write (the single-segment
/// send window returns 0 while unacked data is in flight; a short recv poll
/// drives the ACKs).
const WRITE_STALL_MAX: u32 = 8;

fn tcp_read(fd: i32, buf: &mut [u8]) -> io::Result<usize> {
    for _ in 0..READ_RETRIES {
        match read(fd, buf) {
            Ok(0) => continue,
            Ok(n) => return Ok(n),
            Err(e) => return Err(e),
        }
    }
    Ok(0)
}

fn tcp_write(fd: i32, buf: &[u8]) -> io::Result<usize> {
    let mut off = 0usize;
    let mut stalls = 0u32;
    while off < buf.len() {
        match write(fd, &buf[off..]) {
            Ok(n) if n > 0 => {
                off += n;
                stalls = 0;
            }
            Ok(_) => {
                stalls += 1;
                if stalls > WRITE_STALL_MAX {
                    break;
                }
                let mut tmp = [0u8; 64];
                let _ = read(fd, &mut tmp);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(off)
}

#[derive(Debug)]
pub struct TcpStream {
    inner: FileDesc,
}

impl TcpStream {
    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        each_addr(addr, |addr| {
            let fd = tcp_socket_fd()?;
            let ip = to_ip(addr)?;
            match minix_std::net::connect(fd, ip, addr.port()) {
                Ok(()) => Ok(TcpStream { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
                Err(e) => {
                    let _ = minix_std::net::close(fd);
                    Err(minix_err(e))
                }
            }
        })
    }

    pub fn connect_timeout(addr: &SocketAddr, _timeout: Duration) -> io::Result<TcpStream> {
        let fd = tcp_socket_fd()?;
        let ip = to_ip(addr)?;
        match minix_std::net::connect(fd, ip, addr.port()) {
            Ok(()) => Ok(TcpStream { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
            Err(e) => {
                let _ = minix_std::net::close(fd);
                Err(minix_err(e))
            }
        }
    }

    pub fn set_read_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn set_write_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        unsupported()
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        unsupported()
    }

    pub fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> {
        unsupported()
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        tcp_read(self.inner.as_raw_fd(), buf)
    }

    pub fn read_buf(&self, cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        crate::io::default_read_buf(|buf| self.read(buf), cursor)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        // Single-buffer I/O: read into the first non-empty slice.
        for buf in bufs {
            if !buf.is_empty() {
                return self.read(buf);
            }
        }
        Ok(0)
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        tcp_write(self.inner.as_raw_fd(), buf)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut written = 0usize;
        for buf in bufs {
            written += self.write(buf)?;
        }
        Ok(written)
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        peer_addr(self.inner.as_raw_fd())
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        socket_addr(self.inner.as_raw_fd())
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Read => minix_std::net::SHUT_RD,
            Shutdown::Write => minix_std::net::SHUT_WR,
            Shutdown::Both => minix_std::net::SHUT_RDWR,
        };
        minix_std::net::shutdown(self.inner.as_raw_fd(), how).map_err(minix_err)
    }

    pub fn duplicate(&self) -> io::Result<TcpStream> {
        duplicate(self.inner.as_raw_fd()).map(|inner| TcpStream { inner })
    }

    pub fn set_linger(&self, _timeout: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn linger(&self) -> io::Result<Option<Duration>> {
        unsupported()
    }

    pub fn set_keepalive(&self, _keepalive: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn keepalive(&self) -> io::Result<bool> {
        unsupported()
    }

    pub fn set_nodelay(&self, _nodelay: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        unsupported()
    }

    pub fn set_ttl(&self, _ttl: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        unsupported()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        unsupported()
    }

    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        unsupported()
    }
}

#[derive(Debug)]
pub struct TcpListener {
    inner: FileDesc,
}

impl TcpListener {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<TcpListener> {
        each_addr(addr, |addr| {
            let fd = tcp_socket_fd()?;
            let ip = to_ip(addr)?;
            let r = minix_std::net::bind(fd, ip, addr.port())
                .and_then(|()| minix_std::net::listen(fd, 128));
            match r {
                Ok(()) => Ok(TcpListener { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
                Err(e) => {
                    let _ = minix_std::net::close(fd);
                    Err(minix_err(e))
                }
            }
        })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        socket_addr(self.inner.as_raw_fd())
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let inner = accept(self.inner.as_raw_fd())?;
        let peer = match peer_addr(inner.as_raw_fd()) {
            Ok(addr) => addr,
            Err(_) => SocketAddr::from(([0; 4], 0)),
        };
        Ok((TcpStream { inner }, peer))
    }

    pub fn duplicate(&self) -> io::Result<TcpListener> {
        duplicate(self.inner.as_raw_fd()).map(|inner| TcpListener { inner })
    }

    pub fn set_ttl(&self, _ttl: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        unsupported()
    }

    pub fn set_only_v6(&self, _only_v6: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn only_v6(&self) -> io::Result<bool> {
        unsupported()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        unsupported()
    }

    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        unsupported()
    }
}

#[derive(Debug)]
pub struct UdpSocket {
    inner: FileDesc,
}

impl UdpSocket {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<UdpSocket> {
        each_addr(addr, |addr| {
            let fd = udp_socket_fd()?;
            let ip = to_ip(addr)?;
            match minix_std::net::bind(fd, ip, addr.port()) {
                Ok(()) => Ok(UdpSocket { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
                Err(e) => {
                    let _ = minix_std::net::close(fd);
                    Err(minix_err(e))
                }
            }
        })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        unsupported()
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        unsupported()
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        // SAFETY: `buf` is a valid mutable byte slice.
        let (n, addr) =
            unsafe { minix_std::net::recvfrom(self.inner.as_raw_fd(), buf) }.map_err(minix_err)?;
        Ok((n as usize, to_sockaddr(addr)))
    }

    pub fn peek_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        unsupported()
    }

    pub fn send_to(&self, buf: &[u8], addr: &SocketAddr) -> io::Result<usize> {
        let ip = to_ip(addr)?;
        let dest = minix_std::net::SocketAddr::new(ip, addr.port());
        // SAFETY: `buf` is a valid byte slice.
        let n = unsafe { minix_std::net::sendto(self.inner.as_raw_fd(), buf, Some(dest)) }
            .map_err(minix_err)?;
        Ok(n as usize)
    }

    pub fn duplicate(&self) -> io::Result<UdpSocket> {
        duplicate(self.inner.as_raw_fd()).map(|inner| UdpSocket { inner })
    }

    pub fn set_read_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn set_write_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        unsupported()
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        unsupported()
    }

    pub fn set_broadcast(&self, _broadcast: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn broadcast(&self) -> io::Result<bool> {
        unsupported()
    }

    pub fn set_multicast_loop_v4(&self, _val: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        unsupported()
    }

    pub fn set_multicast_ttl_v4(&self, _val: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        unsupported()
    }

    pub fn set_multicast_loop_v6(&self, _val: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        unsupported()
    }

    pub fn join_multicast_v4(&self, _addr: &Ipv4Addr, _iface: &Ipv4Addr) -> io::Result<()> {
        unsupported()
    }

    pub fn join_multicast_v6(&self, _addr: &Ipv6Addr, _iface: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn leave_multicast_v4(&self, _addr: &Ipv4Addr, _iface: &Ipv4Addr) -> io::Result<()> {
        unsupported()
    }

    pub fn leave_multicast_v6(&self, _addr: &Ipv6Addr, _iface: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn set_ttl(&self, _ttl: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        unsupported()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        unsupported()
    }

    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        // SAFETY: `buf` is a valid mutable byte slice.
        let (n, _) =
            unsafe { minix_std::net::recvfrom(self.inner.as_raw_fd(), buf) }.map_err(minix_err)?;
        Ok(n as usize)
    }

    pub fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> {
        unsupported()
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        // SAFETY: `buf` is a valid byte slice.
        let n = unsafe { minix_std::net::sendto(self.inner.as_raw_fd(), buf, None) }
            .map_err(minix_err)?;
        Ok(n as usize)
    }

    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        each_addr(addr, |addr| {
            let ip = to_ip(addr)?;
            minix_std::net::connect(self.inner.as_raw_fd(), ip, addr.port()).map_err(minix_err)
        })
    }
}

pub struct LookupHost(!);

impl Iterator for LookupHost {
    type Item = SocketAddr;
    fn next(&mut self) -> Option<SocketAddr> {
        self.0
    }
}

pub fn lookup_host(_host: &str, _port: u16) -> io::Result<LookupHost> {
    unsupported()
}
