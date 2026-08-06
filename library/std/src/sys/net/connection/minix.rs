//! Socket API over the Minix `/dev/tcp` and `/dev/udp` clone-minor devices.
//!
//! A socket is an `open("/dev/tcp")` (SOCK_STREAM) or `open("/dev/udp")`
//! (SOCK_DGRAM) descriptor. `bind`/`connect`/`listen`/`accept` are the
//! reference `NWIO*` ioctls carrying the `nwio_*` option structs; `send` and
//! `recv` are plain `write`/`read` on the descriptor (a whole datagram for
//! UDP, a byte stream for TCP). The wire protocol mirrors
//! `.refs/minixrs/crates/net/`.

use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, SocketAddrV4, ToSocketAddrs};
use crate::os::fd::FromRawFd;
use crate::path::Path;
use crate::sys::fd::FileDesc;
use crate::sys::net::connection::each_addr;
use crate::sys::pal::minix::fs::{close, ioctl, open, read, write};
use crate::sys::pal::minix::syscall;
use crate::sys::unsupported;
use crate::time::Duration;

// ---- NetBSD-style ioctl request encoding (mirrors `crates/net/src/lib.rs`) ----

const fn ioc_encode(dir: u32, group: u8, num: u8, size: usize) -> u32 {
    dir | ((size as u32) & 0x1fff) << 16 | ((group as u32) << 8) | (num as u32)
}

// Socket ioctl request codes (`net/gen/udp_io.h`, `net/gen/tcp_io.h`).
const NWIOSUDPOPT: u32 = ioc_encode(0x8000_0000, b'n', 64, 16); // set udp opts
const NWIOSTCPCONF: u32 = ioc_encode(0x8000_0000, b'n', 48, 16); // set tcp conf
const NWIOGTCPCONF: u32 = ioc_encode(0x4000_0000, b'n', 49, 16); // get tcp conf
const NWIOTCPCONN: u32 = ioc_encode(0x8000_0000, b'n', 50, 8); // connect
const NWIOTCPLISTENQ: u32 = ioc_encode(0x8000_0000, b'n', 57, 4); // listen(2)
const NWIOGTCPCOOKIE: u32 = ioc_encode(0x4000_0000, b'n', 58, 16); // accept cookie
const NWIOTCPACCEPTTO: u32 = ioc_encode(0x8000_0000, b'n', 59, 16); // accept handoff

// NWUO_* UDP option flags (`net/gen/udp_io.h`).
const NWUO_LP_SEL: u32 = 0x0004;
const NWUO_LP_SET: u32 = 0x0008;
const NWUO_EN_LOC: u32 = 0x0010;
const NWUO_RP_SET: u32 = 0x0100;
const NWUO_RA_SET: u32 = 0x0200;
const NWUO_RWDATONLY: u32 = 0x0000_1000;

// NWTC_* TCP config flags and TCF_* connect flags (`net/gen/tcp_io.h`).
const NWTC_LP_SEL: u32 = 0x0030;
const NWTC_LP_SET: u32 = 0x0020;
const NWTC_SET_RA: u32 = 0x0100;
const NWTC_SET_RP: u32 = 0x0200;
const TCF_DEFAULT: u32 = 0;

/// `/dev/udp` option struct (`struct nwio_udpopt`): native-endian, 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NwioUdpOpt {
    nwuo_flags: u32,
    nwuo_locport: u16,
    nwuo_remport: u16,
    nwuo_locaddr: u32,
    nwuo_remaddr: u32,
}

/// `/dev/tcp` config struct (`struct nwio_tcpconf`): native-endian, 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NwioTcpConf {
    nwtc_flags: u32,
    nwtc_locaddr: u32,
    nwtc_remaddr: u32,
    nwtc_locport: u16,
    nwtc_remport: u16,
}

/// `/dev/tcp` connect struct (`struct nwio_tcpcl`): 8 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NwioTcpCl {
    nwtcl_flags: u32,
    nwtcl_ttl: u32,
}

/// Accept cookie (`struct tcp_cookie`): names a fresh socket to the listener.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TcpCookie {
    tc_ref: u32,
    tc_secret: [u8; 12],
}

/// Open a `/dev/tcp` socket descriptor. The local port is auto-assigned at
/// connect/bind time.
fn tcp_socket_fd() -> io::Result<i32> {
    open(Path::new("/dev/tcp"), syscall::O_RDWR, 0)
}

/// Open a `/dev/udp` socket descriptor and implicitly bind it to an
/// ephemeral local port on the local address.
fn udp_socket_fd() -> io::Result<i32> {
    let fd = open(Path::new("/dev/udp"), syscall::O_RDWR, 0)?;
    let opt =
        NwioUdpOpt { nwuo_flags: NWUO_LP_SEL | NWUO_EN_LOC | NWUO_RWDATONLY, ..Default::default() };
    // SAFETY: `opt` is a valid 16-byte NwioUdpOpt buffer.
    if let Err(e) = unsafe { ioctl(fd, NWIOSUDPOPT, &opt as *const NwioUdpOpt as *mut u8) } {
        let _ = close(fd);
        return Err(e);
    }
    Ok(fd)
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

/// Bind a TCP socket to a local port (INADDR_ANY). A port of 0 asks the net
/// server for an ephemeral port.
fn tcp_bind(fd: i32, addr: &SocketAddr) -> io::Result<()> {
    let _ = to_ip(addr)?;
    let (flags, locport) =
        if addr.port() == 0 { (NWTC_LP_SEL, 0) } else { (NWTC_LP_SET, addr.port()) };
    let conf = NwioTcpConf { nwtc_flags: flags, nwtc_locport: locport, ..Default::default() };
    // SAFETY: `conf` is a valid 16-byte NwioTcpConf buffer.
    unsafe { ioctl(fd, NWIOSTCPCONF, &conf as *const NwioTcpConf as *mut u8) }.map(drop)
}

/// Put a bound TCP socket into the listening state.
fn tcp_listen(fd: i32, backlog: i32) -> io::Result<()> {
    // SAFETY: `backlog` is a valid 4-byte i32 buffer.
    unsafe { ioctl(fd, NWIOTCPLISTENQ, &backlog as *const i32 as *mut u8) }.map(drop)
}

/// Run the three-way handshake: set the remote address/port (auto local
/// port), then block until established.
fn tcp_connect(fd: i32, addr: &SocketAddr) -> io::Result<()> {
    let ip = to_ip(addr)?;
    let conf = NwioTcpConf {
        nwtc_flags: NWTC_LP_SEL | NWTC_SET_RA | NWTC_SET_RP,
        nwtc_remaddr: u32::from_be_bytes(ip),
        nwtc_remport: addr.port(),
        ..Default::default()
    };
    // SAFETY: `conf` is a valid 16-byte NwioTcpConf buffer.
    unsafe { ioctl(fd, NWIOSTCPCONF, &conf as *const NwioTcpConf as *mut u8) }?;
    let cl = NwioTcpCl { nwtcl_flags: TCF_DEFAULT, nwtcl_ttl: 0 };
    // SAFETY: `cl` is a valid 8-byte NwioTcpCl buffer.
    unsafe { ioctl(fd, NWIOTCPCONN, &cl as *const NwioTcpCl as *mut u8) }.map(drop)
}

/// Bind a UDP socket to a local address/port (a port of 0 is ephemeral).
fn udp_bind(fd: i32, addr: &SocketAddr) -> io::Result<()> {
    let ip = to_ip(addr)?;
    let (flags, locport) =
        if addr.port() == 0 { (NWUO_LP_SEL, 0) } else { (NWUO_LP_SET, addr.port()) };
    let opt = NwioUdpOpt {
        nwuo_flags: flags | NWUO_EN_LOC | NWUO_RWDATONLY,
        nwuo_locport: locport,
        nwuo_locaddr: u32::from_be_bytes(ip),
        ..Default::default()
    };
    // SAFETY: `opt` is a valid 16-byte NwioUdpOpt buffer.
    unsafe { ioctl(fd, NWIOSUDPOPT, &opt as *const NwioUdpOpt as *mut u8) }.map(drop)
}

/// Set the default destination and the receive filter of a UDP socket.
fn udp_connect(fd: i32, addr: &SocketAddr) -> io::Result<()> {
    let ip = to_ip(addr)?;
    let opt = NwioUdpOpt {
        nwuo_flags: NWUO_RP_SET | NWUO_RA_SET | NWUO_RWDATONLY,
        nwuo_remport: addr.port(),
        nwuo_remaddr: u32::from_be_bytes(ip),
        ..Default::default()
    };
    // SAFETY: `opt` is a valid 16-byte NwioUdpOpt buffer.
    unsafe { ioctl(fd, NWIOSUDPOPT, &opt as *const NwioUdpOpt as *mut u8) }.map(drop)
}

/// Read the current TCP configuration (`NWIOGTCPCONF`).
fn tcp_conf(fd: i32) -> io::Result<NwioTcpConf> {
    let mut conf = NwioTcpConf::default();
    // SAFETY: `conf` is a valid 16-byte NwioTcpConf buffer.
    unsafe { ioctl(fd, NWIOGTCPCONF, &mut conf as *mut NwioTcpConf as *mut u8) }?;
    Ok(conf)
}

fn peer_addr(fd: i32) -> io::Result<SocketAddr> {
    let conf = tcp_conf(fd)?;
    Ok(from_ip_port(u32::to_be_bytes(conf.nwtc_remaddr), conf.nwtc_remport))
}

fn socket_addr(fd: i32) -> io::Result<SocketAddr> {
    let conf = tcp_conf(fd)?;
    Ok(from_ip_port(u32::to_be_bytes(conf.nwtc_locaddr), conf.nwtc_locport))
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

/// Accept the next pending connection: open a fresh `/dev/tcp` fd, obtain
/// its accept cookie, and transfer the pending connection to it. Blocks
/// (retrying EAGAIN, which the net server returns when its bounded accept
/// poll expires) until a connection arrives.
fn accept(fd: i32) -> io::Result<FileDesc> {
    let s1 = tcp_socket_fd()?;
    let mut cookie = TcpCookie::default();
    // SAFETY: `cookie` is a valid 16-byte TcpCookie buffer.
    if let Err(e) = unsafe { ioctl(s1, NWIOGTCPCOOKIE, &mut cookie as *mut TcpCookie as *mut u8) } {
        let _ = close(s1);
        return Err(e);
    }
    loop {
        // SAFETY: `cookie` is a valid 16-byte TcpCookie buffer.
        match unsafe { ioctl(fd, NWIOTCPACCEPTTO, &cookie as *const TcpCookie as *mut u8) } {
            Ok(_) => return Ok(unsafe { FileDesc::from_raw_fd(s1) }),
            Err(e) if e.raw_os_error() == Some(syscall::EAGAIN) => continue,
            Err(e) => {
                let _ = close(s1);
                return Err(e);
            }
        }
    }
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
            match tcp_connect(fd, addr) {
                Ok(()) => Ok(TcpStream { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
                Err(e) => {
                    let _ = close(fd);
                    Err(e)
                }
            }
        })
    }

    pub fn connect_timeout(addr: &SocketAddr, _timeout: Duration) -> io::Result<TcpStream> {
        let fd = tcp_socket_fd()?;
        match tcp_connect(fd, addr) {
            Ok(()) => Ok(TcpStream { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
            Err(e) => {
                let _ = close(fd);
                Err(e)
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

    pub fn shutdown(&self, _how: Shutdown) -> io::Result<()> {
        unsupported()
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
            match tcp_bind(fd, addr).and_then(|()| tcp_listen(fd, 128)) {
                Ok(()) => Ok(TcpListener { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
                Err(e) => {
                    let _ = close(fd);
                    Err(e)
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
            match udp_bind(fd, addr) {
                Ok(()) => Ok(UdpSocket { inner: unsafe { FileDesc::from_raw_fd(fd) } }),
                Err(e) => {
                    let _ = close(fd);
                    Err(e)
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

    pub fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        unsupported()
    }

    pub fn peek_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        unsupported()
    }

    pub fn send_to(&self, _buf: &[u8], _addr: &SocketAddr) -> io::Result<usize> {
        unsupported()
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
        read(self.inner.as_raw_fd(), buf)
    }

    pub fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> {
        unsupported()
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        write(self.inner.as_raw_fd(), buf)
    }

    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        each_addr(addr, |addr| udp_connect(self.inner.as_raw_fd(), addr))
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
