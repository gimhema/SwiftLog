use std::io;
use std::net::UdpSocket;

pub struct UdpRx {
    sock: UdpSocket,
    buf: Box<[u8]>,
}

impl UdpRx {
    pub fn bind(addr: &str, buf_size: usize) -> io::Result<Self> {
        let sock = UdpSocket::bind(addr)?;
        sock.set_nonblocking(true)?;
        Ok(Self { sock, buf: vec![0u8; buf_size].into_boxed_slice() })
    }

    // 논블로킹 단발 수신 한 번만 시도 (없으면 WouldBlock)
    pub fn recv_once<F: FnMut(&[u8]) -> io::Result<()>>(&mut self, mut on_batch: F) {
        match self.sock.recv(&mut self.buf) {
            Ok(n) => { let _ = on_batch(&self.buf[..n]); }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(_e) => {}
        }
    }
}
