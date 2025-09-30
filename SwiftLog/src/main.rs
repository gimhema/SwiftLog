// swiftlog-agent/src/main.rs
use std::collections::VecDeque;
use std::fs::{OpenOptions, File};
use std::io::{self, Read, Write};
use std::net::{UdpSocket, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const UDP_BIND: &str = "127.0.0.1:9501";
const TCP_BIND: &str = "127.0.0.1:9502";
const UDP_BUF: usize = 2048;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

const MAGIC_LE: &[u8;4] = b"SLG1";

struct Conn {
    stream: TcpStream,
    buf: Vec<u8>,
}
impl Conn {
    fn new(s: TcpStream) -> io::Result<Self> {
        s.set_nonblocking(true)?;
        Ok(Self { stream: s, buf: Vec::with_capacity(4096) })
    }
}

struct LogWriter {
    dir: PathBuf,
    base: String,
    file: File,
    written: u64,
}
impl LogWriter {
    fn open(dir: &str, base: &str) -> io::Result<Self> {
        let d = PathBuf::from(dir);
        std::fs::create_dir_all(&d)?;
        let path = d.join(format!("{}.log", base));
        let f = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { dir: d, base: base.into(), file: f, written: 0 })
    }
    fn rotate_if_needed(&mut self) -> io::Result<()> {
        if self.written < MAX_FILE_BYTES { return Ok(()); }
        drop(&self.file);
        let ts = SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_secs();
        let src = self.dir.join(format!("{}.log", self.base));
        let dst = self.dir.join(format!("{}.log.{}", self.base, ts));
        let _ = std::fs::rename(&src, &dst);
        self.file = OpenOptions::new().create(true).append(true)
            .open(self.dir.join(format!("{}.log", self.base)))?;
        self.written = 0;
        Ok(())
    }
    fn write_line(&mut self, line: &[u8]) -> io::Result<()> {
        self.file.write_all(line)?;
        self.file.write_all(b"\n")?;
        self.written += (line.len() + 1) as u64;
        Ok(())
    }
}

fn parse_and_write(batch: &[u8], w: &mut LogWriter) {
    if batch.len() < 8 || &batch[0..4] != MAGIC_LE { return; }
    let count = u16::from_le_bytes([batch[6], batch[7]]) as usize;
    let mut p = 8usize;
    for _ in 0..count {
        if p + 8 + 1 + 2 + 2 > batch.len() { break; }
        let ts = u64::from_le_bytes(batch[p..p+8].try_into().unwrap()); p += 8;
        let level = batch[p]; p += 1;
        let code = u16::from_le_bytes(batch[p..p+2].try_into().unwrap()); p += 2;
        let len  = u16::from_le_bytes(batch[p..p+2].try_into().unwrap()) as usize; p += 2;
        if p + len > batch.len() { break; }
        let msg = &batch[p..p+len]; p += len;

        let mut line = Vec::with_capacity(64 + len);
        // TSV 스타일: ts	level	code	message
        line.extend_from_slice(ts.to_string().as_bytes());
        line.push(b'\t'); line.extend_from_slice(level.to_string().as_bytes());
        line.push(b'\t'); line.extend_from_slice(code.to_string().as_bytes());
        line.push(b'\t'); line.extend_from_slice(msg);
        let _ = w.write_line(&line);
    }
}

fn main() -> io::Result<()> {
    let mut writer = LogWriter::open("logs", "app")?;

    // UDP
    let udp = UdpSocket::bind(UDP_BIND)?;
    udp.set_nonblocking(true)?;
    let mut ubuf = [0u8; UDP_BUF];

    // TCP
    let tl = TcpListener::bind(TCP_BIND)?;
    tl.set_nonblocking(true)?;
    let mut conns: Vec<Conn> = Vec::new();
    let mut dead: VecDeque<usize> = VecDeque::new();

    loop {
        // UDP 수신
        match udp.recv(&mut ubuf) {
            Ok(n) => parse_and_write(&ubuf[..n], &mut writer),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {},
            Err(_) => {}
        }

        // TCP accept
        match tl.accept() {
            Ok((s, _addr)) => if let Ok(c) = Conn::new(s) { conns.push(c); },
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {},
            Err(_) => {}
        }

        // TCP 각 연결에서 읽기
        for (i, c) in conns.iter_mut().enumerate() {
            let mut tmp = [0u8; 4096];
            loop {
                match c.stream.read(&mut tmp) {
                    Ok(0) => { dead.push_back(i); break; } // closed
                    Ok(n) => { c.buf.extend_from_slice(&tmp[..n]); }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => { dead.push_back(i); break; }
                }
            }
            // 프레이밍: [len:u32][Batch]
            let mut offset = 0usize;
            while c.buf.len() >= offset + 4 {
                let len = u32::from_le_bytes(c.buf[offset..offset+4].try_into().unwrap()) as usize;
                if c.buf.len() < offset + 4 + len { break; }
                let start = offset + 4;
                let end = start + len;
                parse_and_write(&c.buf[start..end], &mut writer);
                offset = end;
            }
            if offset > 0 { c.buf.drain(0..offset); }
        }
        // 죽은 연결 제거
        while let Some(i) = dead.pop_front() {
            if i < conns.len() { conns.swap_remove(i); }
        }

        writer.rotate_if_needed()?;
        std::thread::sleep(Duration::from_millis(5));
    }
}
