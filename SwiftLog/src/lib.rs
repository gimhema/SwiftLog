// swiftlog/src/lib.rs
use std::io::{self, Write};
use std::net::{UdpSocket, TcpStream, SocketAddr};
use std::time::{SystemTime, UNIX_EPOCH};
use std::mem;

pub const MAGIC: u32 = 0x31474C53; // 'SLG1' LE
pub const VERSION: u16 = 1;

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum LogLevel { Trace=0, Debug=1, Info=2, Warn=3, Error=4 }

enum Transport {
    Udp { sock: UdpSocket, dst: SocketAddr, max_datagram: usize },
    Tcp { stream: TcpStream, scratch: Vec<u8> },
}

pub struct Logger {
    t: Transport,
    buf: Vec<u8>,        // Batch 누적 버퍼 (BatchHeader 포함)
    count_in_batch: u16,
    dropped: u64,
}

impl Logger {
    pub fn new_udp(dst: SocketAddr) -> io::Result<Self> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_nonblocking(true)?;
        let mut s = Self {
            t: Transport::Udp { sock, dst, max_datagram: 1300 },
            buf: Vec::with_capacity(1400),
            count_in_batch: 0,
            dropped: 0,
        };
        s.begin_batch();
        Ok(s)
    }

    pub fn new_tcp(dst: SocketAddr) -> io::Result<Self> {
        let stream = TcpStream::connect(dst)?;
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?; // Nagle off: 지연 감소
        let mut s = Self {
            t: Transport::Tcp { stream, scratch: Vec::with_capacity(1400) },
            buf: Vec::with_capacity(1400),
            count_in_batch: 0,
            dropped: 0,
        };
        s.begin_batch();
        Ok(s)
    }

    fn begin_batch(&mut self) {
        self.buf.clear();
        self.count_in_batch = 0;
        self.buf.extend_from_slice(&MAGIC.to_le_bytes());
        self.buf.extend_from_slice(&VERSION.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // count placeholder
    }

    #[inline] fn now_ms() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
    }

    pub fn log(&mut self, level: LogLevel, code: u16, msg: &str) {
        let ts = Self::now_ms();
        let m = msg.as_bytes();
        let len = m.len().min(u16::MAX as usize) as u16;

        let need = 8 + 1 + 2 + 2 + (len as usize);
        let headroom = 2;

        let max_datagram = match self.t {
            Transport::Udp { max_datagram, .. } => max_datagram,
            Transport::Tcp { .. } => usize::MAX, // TCP는 배치 크기 자유(단, 너무 크지 않게 운용)
        };

        if self.buf.len() + need + headroom > max_datagram {
            let _ = self.flush(); // 실패 시 드롭
        }

        self.buf.extend_from_slice(&ts.to_le_bytes());
        self.buf.push(level as u8);
        self.buf.extend_from_slice(&code.to_le_bytes());
        self.buf.extend_from_slice(&len.to_le_bytes());
        self.buf.extend_from_slice(&m[..len as usize]);

        self.count_in_batch = self.count_in_batch.saturating_add(1);

        if let Transport::Udp { max_datagram, .. } = self.t {
            if self.buf.len() >= max_datagram - 64 {
                let _ = self.flush();
            }
        }
    }

    // flush() 전체를 교체하거나, UDP 분기만 이렇게 수정
pub fn flush(&mut self) -> io::Result<()> {
    if self.count_in_batch == 0 { return Ok(()); }
    // count 패치
    let c = self.count_in_batch.to_le_bytes();
    self.buf[6] = c[0]; self.buf[7] = c[1];

    let res = match &mut self.t {
        Transport::Udp { sock, dst, .. } => {
            // dst: &mut SocketAddr 이므로 값 복사로 넘김
            sock.send_to(&self.buf, *dst).map(|_| ())
            //              ▲ 여기!
        }
        Transport::Tcp { stream, scratch } => {
            scratch.clear();
            let len = self.buf.len() as u32;
            scratch.extend_from_slice(&len.to_le_bytes());
            scratch.extend_from_slice(&self.buf);

            match stream.write(&scratch) {
                Ok(w) if w == scratch.len() => Ok(()),
                Ok(_partial) => Err(io::Error::new(io::ErrorKind::WouldBlock, "partial write")),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Err(e),
                Err(e) => Err(e),
            }
        }
    };

    if res.is_err() {
        self.dropped += self.count_in_batch as u64;
    }
    self.begin_batch();
    res
}


    pub fn dropped_count(&self) -> u64 { self.dropped }
}
