use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{channel, Receiver},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// tail 스레드 → GUI 이벤트
pub enum TailEvent {
    Line(String),
    Rotated,
    Error(String),
}

/// tail 제어句
pub struct TailCtrl {
    pub rx: Receiver<TailEvent>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TailCtrl {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

pub fn start_tail(path: PathBuf) -> io::Result<TailCtrl> {
    let (tx, rx) = channel::<TailEvent>();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_c = stop.clone();

    let handle = thread::Builder::new()
        .name("swiftlog-gui-tail".into())
        .spawn(move || {
            let mut cur_path = path.clone();

            let mut open_and_seek_end = |p: &PathBuf| -> io::Result<(File, u64)> {
                let mut f = File::open(p)?;
                let sz = f.metadata()?.len();
                f.seek(SeekFrom::Start(sz))?;
                Ok((f, sz))
            };

            // 처음 파일 열기 (없으면 대기)
            let mut file = match open_and_seek_end(&cur_path) {
                Ok((f, _)) => f,
                Err(e) => {
                    let _ = tx.send(TailEvent::Error(format!("open error: {}", e)));
                    loop {
                        if stop_c.load(Ordering::Relaxed) {
                            return;
                        }
                        match open_and_seek_end(&cur_path) {
                            Ok((f, _)) => break f,
                            Err(_) => thread::sleep(Duration::from_millis(500)),
                        }
                    }
                }
            };

            let mut reader = BufReader::new(file);

            loop {
                if stop_c.load(Ordering::Relaxed) {
                    break;
                }

                let mut buf = String::new();
                match reader.read_line(&mut buf) {
                    Ok(0) => {
                        // EOF: 잠시 대기, 롤링은 간단히 재열기로 처리
                        thread::sleep(Duration::from_millis(200));
                        if let Ok((f, _)) = open_and_seek_end(&cur_path) {
                            reader = BufReader::new(f);
                            let _ = tx.send(TailEvent::Rotated);
                        }
                    }
                    Ok(_) => {
                        if !buf.is_empty() {
                            if buf.ends_with('\n') {
                                buf.pop();
                                if buf.ends_with('\r') {
                                    buf.pop();
                                }
                            }
                            let _ = tx.send(TailEvent::Line(buf));
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(TailEvent::Error(format!("read error: {}", e)));
                        thread::sleep(Duration::from_millis(500));
                    }
                }
            }
        })?;

    Ok(TailCtrl {
        rx,
        stop,
        handle: Some(handle),
    })
}
