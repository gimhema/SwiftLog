use crate::parser::parse_and_write;
use std::collections::VecDeque;
use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

pub struct Conn {
    stream: TcpStream,
    buf: Vec<u8>, // 누적 read 버퍼
}
impl Conn {
    fn new(s: TcpStream) -> io::Result<Self> {
        s.set_nonblocking(true)?;
        s.set_nodelay(true)?; // 지연 줄이기
        Ok(Self { stream: s, buf: Vec::with_capacity(4096) })
    }

    // 이 연결에서 읽을 수 있는 만큼 읽고, [len|batch] 파싱
    pub fn poll_read<F: FnMut(&[u8]) -> io::Result<()>>(&mut self, mut on_batch: F) -> bool {
        let mut tmp = [0u8; 4096];
        loop {
            match self.stream.read(&mut tmp) {
                Ok(0) => return false, // closed
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => return false,
            }
        }

        let mut offset = 0usize;
        while self.buf.len() >= offset + 4 {
            let len = u32::from_le_bytes(self.buf[offset..offset+4].try_into().unwrap()) as usize;
            if self.buf.len() < offset + 4 + len { break; }
            let start = offset + 4;
            let end = start + len;
            let _ = on_batch(&self.buf[start..end]);
            offset = end;
        }
        if offset > 0 { self.buf.drain(0..offset); }
        true
    }
}

pub struct TcpRx {
    listener: TcpListener,
    conns: Vec<Conn>,
    dead: VecDeque<usize>,
}
impl TcpRx {
    pub fn bind(addr: &str) -> io::Result<Self> {
        let l = TcpListener::bind(addr)?;
        l.set_nonblocking(true)?;
        Ok(Self { listener: l, conns: Vec::new(), dead: VecDeque::new() })
    }

    pub fn accept_once(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((s, _addr)) => {
                    if let Ok(c) = Conn::new(s) { self.conns.push(c); }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_e) => break,
            }
        }
    }

    pub fn poll_once<F: FnMut(&[u8]) -> io::Result<()>>(&mut self, mut on_batch: F) {
        for (i, c) in self.conns.iter_mut().enumerate() {
            if !c.poll_read(&mut on_batch) {
                self.dead.push_back(i);
            }
        }
        while let Some(i) = self.dead.pop_front() {
            if i < self.conns.len() { self.conns.swap_remove(i); }
        }
    }
}
