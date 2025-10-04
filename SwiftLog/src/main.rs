mod proto;
mod parser;
mod writer;
mod udp;
mod tcp;
mod console;
mod console_command;

use crate::proto::{UDP_BUF_SIZE};
use crate::writer::LogWriter;
use crate::udp::UdpRx;
use crate::tcp::TcpRx;

use std::io;
use std::time::{Duration, SystemTime};

fn main() -> io::Result<()> {
    // 설정(필요시 env/CLI로 확장)
    let udp_bind = "127.0.0.1:9501";
    let tcp_bind = "127.0.0.1:9502";

    let mut writer = LogWriter::open("logs", "app")?;

    // UDP/TCP 초기화
    let mut udp = UdpRx::bind(udp_bind, UDP_BUF_SIZE)?;
    let mut tcp = TcpRx::bind(tcp_bind)?;

    let mut last_housekeep = SystemTime::now();

    loop {
        // UDP 단발 수신
        // UDP 단발 수신
        udp.recv_once(|datagram| {
            parser::parse_and_write(datagram, |line| writer.write_line(line));
            Ok(()) // <-- 추가
        });

        // TCP accept & 읽기
        tcp.accept_once();
        tcp.poll_once(|batch| {
            parser::parse_and_write(batch, |line| writer.write_line(line));
            Ok(()) // <-- 추가
        });


        // 주기적 하우스키핑
        if last_housekeep.elapsed().unwrap_or(Duration::from_secs(0)) > Duration::from_millis(200) {
            let _ = writer.rotate_if_needed();
            last_housekeep = SystemTime::now();
        }

        std::thread::sleep(Duration::from_millis(5));
    }
}
