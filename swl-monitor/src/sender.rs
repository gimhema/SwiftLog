use crate::protocol::{encode_batch, LogLevel};
use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
    mpsc::Sender as MpscSender,
};
use std::thread;
use std::time::Duration;

/// 전송 방식
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Udp,
    Tcp,
}

/// 전송 스레드 제어句
pub struct SendCtrl {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl SendCtrl {
    pub fn is_running(&self) -> bool {
        self.handle.is_some() && !self.stop.load(Ordering::Relaxed)
    }
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// 전송 통계
pub struct SendStats {
    pub sent: u64,
    pub errors: u64,
}

/// 테스트 로그 전송 시작 (백그라운드 스레드)
pub fn start_sending(
    transport: Transport,
    addr: SocketAddr,
    msg_template: String,
    level: LogLevel,
    code: u16,
    rate_per_sec: f64,
    stats_tx: MpscSender<SendStats>,
) -> io::Result<SendCtrl> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_c = stop.clone();

    let handle = thread::Builder::new()
        .name("swiftlog-gui-sender".into())
        .spawn(move || {
            let mut sent: u64 = 0;
            let mut errors: u64 = 0;

            let sleep_ns = if rate_per_sec <= 0.0 {
                0
            } else {
                (1_000_000_000f64 / rate_per_sec).max(0.0) as u64
            };

            match transport {
                Transport::Udp => {
                    let sock = match UdpSocket::bind("0.0.0.0:0") {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    while !stop_c.load(Ordering::Relaxed) {
                        let payload = format!("{} #{sent}", msg_template);
                        let batch = encode_batch(&[(level, code, &payload)]);
                        match sock.send_to(&batch, addr) {
                            Ok(_) => sent += 1,
                            Err(_) => errors += 1,
                        }
                        if sleep_ns > 0 {
                            thread::sleep(Duration::from_nanos(sleep_ns));
                        } else {
                            thread::yield_now();
                        }
                        if sent % 50 == 0 {
                            let _ = stats_tx.send(SendStats { sent, errors });
                        }
                    }
                }
                Transport::Tcp => {
                    let mut stream = match TcpStream::connect(addr) {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let _ = stream.set_nodelay(true);

                    while !stop_c.load(Ordering::Relaxed) {
                        let payload = format!("{} #{sent}", msg_template);
                        let batch = encode_batch(&[(level, code, &payload)]);
                        let len = batch.len() as u32;

                        let mut frame = Vec::with_capacity(4 + batch.len());
                        frame.extend_from_slice(&len.to_le_bytes());
                        frame.extend_from_slice(&batch);

                        match stream.write_all(&frame) {
                            Ok(_) => sent += 1,
                            Err(_) => errors += 1,
                        }
                        if sleep_ns > 0 {
                            thread::sleep(Duration::from_nanos(sleep_ns));
                        } else {
                            thread::yield_now();
                        }
                        if sent % 50 == 0 {
                            let _ = stats_tx.send(SendStats { sent, errors });
                        }
                    }
                }
            }

            let _ = stats_tx.send(SendStats { sent, errors });
        })?;

    Ok(SendCtrl { stop, handle: Some(handle) })
}
