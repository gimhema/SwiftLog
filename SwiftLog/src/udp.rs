use std::io;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicU64, Ordering};

// pub static RECV_PACKET_COUNT: AtomicU64 = AtomicU64::new(0);
pub static RECV_NORMAL_PACKET_COUNT: AtomicU64 = AtomicU64::new(0);
// pub static ERROR_PACKET_COUNT: AtomicU64 = AtomicU64::new(0);
// pub static WOULD_BACK_PACKET_COUNT: AtomicU64 = AtomicU64::new(0);

// pub fn increase_count() {
//     RECV_PACKET_COUNT.fetch_add(1, Ordering::Relaxed);
// }
// pub fn get_count() -> u64 {
//     RECV_PACKET_COUNT.load(Ordering::Relaxed)
// }

pub fn increase_normal_count() {
    RECV_NORMAL_PACKET_COUNT.fetch_add(1, Ordering::Relaxed);
}
pub fn get_normal_count() -> u64 {
    RECV_NORMAL_PACKET_COUNT.load(Ordering::Relaxed)
}

// pub fn increase_err_count() {
//     ERROR_PACKET_COUNT.fetch_add(1, Ordering::Relaxed);
// }
// pub fn get_err_count() -> u64 {
//     ERROR_PACKET_COUNT.load(Ordering::Relaxed)
// }

// pub fn increase_would_back_count() {
//     WOULD_BACK_PACKET_COUNT.fetch_add(1, Ordering::Relaxed);
// }
// pub fn get_would_back_count() -> u64 {
//     WOULD_BACK_PACKET_COUNT.load(Ordering::Relaxed)
// }

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

        // RECV_PACKET_COUNT.fetch_add(1, Ordering::Relaxed);


        match self.sock.recv(&mut self.buf) {
            Ok(n) => {
                increase_normal_count();
                 let _ = on_batch(&self.buf[..n]); 
                }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
//                increase_would_back_count();
            }
            Err(_e) => {
                // increase_err_count();
            }
        }
    }
}
