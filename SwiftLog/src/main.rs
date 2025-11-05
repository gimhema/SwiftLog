use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::Arc;

use std::sync::mpsc;
use std::thread;

mod proto;
mod parser;
mod writer;
mod udp;
mod tcp;
mod console;            // 기존
mod console_command;    // 기존

mod log_domain;         // 새 모듈
mod log_store;          // 새 모듈
mod console_select;     // 새 모듈
mod console_degsign;
mod backup_quota;       // 새 모듈
mod logger;

use crate::proto::UDP_BUF_SIZE;
use crate::writer::LogWriter;
use crate::udp::UdpRx;
use crate::tcp::TcpRx;

use crate::log_store::LogStore;
use crate::console_select::ConsoleSelect;
use crate::console_degsign::render_home;

// ───────────────────────────────────────────────────────────────────────────────
// 자동 백업 설정 (필요시 값만 조정)
// - AUTO_BACKUP_ENABLED : 자동백업 ON/OFF
// - AUTO_BACKUP_INTERVAL_SECS : 자동백업 간격(초)
// - AUTO_BACKUP_QUERY_STR : 자동백업에 사용할 선택 쿼리 문자열(명령 형식과 동일)
//   예) "latest limit=10000", ""(전체), "level>=Warn latest limit=5000" 등
// 파일명은 외부 크레이트 없이 epoch 초 기반으로 생성합니다.
// ───────────────────────────────────────────────────────────────────────────────
const AUTO_BACKUP_ENABLED: bool = true;
const AUTO_BACKUP_INTERVAL_SECS: u64 = 60;
const AUTO_BACKUP_QUERY_STR: &str = "latest limit=10000";


fn main() -> io::Result<()> {
    // 인메모리 로그 저장소 & 콘솔 셀렉터
    let store = Arc::new(LogStore::with_capacity(200_000, 8)); // 총 20만 건, 8 샤드
    let console_select = Arc::new(ConsoleSelect::new(store.clone()));

    render_home("SwiftLog", env!("CARGO_PKG_VERSION"), store.len(), true);

    // 콘솔 입력용 채널 & 스레드
    let (tx_cmd, rx_cmd) = mpsc::channel::<String>();
    {
        let tx_cmd = tx_cmd.clone();
        thread::spawn(move || {
            let stdin = io::stdin();
            loop {
                let mut line = String::new();
                if stdin.read_line(&mut line).is_ok() {
                    let line = line.trim().to_string();
                    if !line.is_empty() {
                        // 실패해도 무시하고 계속(콘솔 끊김 방지)
                        let _ = tx_cmd.send(line);
                    }
                }
            }
        });
    }

    // 네트워크 바인딩
    let udp_bind = "127.0.0.1:9050";
    let tcp_bind = "127.0.0.1:9502";

    let mut writer = LogWriter::open("logs", "app")?;

    let mut udp = UdpRx::bind(udp_bind, UDP_BUF_SIZE)?;
    let mut tcp = TcpRx::bind(tcp_bind)?;

    let mut last_housekeep = SystemTime::now();

    // 자동 백업 타이머
    let mut last_auto_backup = SystemTime::now();

    // 메인 루프
    loop {
        // ── 콘솔 명령 처리(논블로킹) ────────────────────────────────────────────────
        // 한 틱에 누적된 명령들을 최대한 소진
        while let Ok(cmd) = rx_cmd.try_recv() {
            dispatch_console_command(&cmd, &console_select);
        }

        udp.recv_once(|datagram| {
            parser::parse_and_store_and_write(datagram, Some(&store), |line| writer.write_line(line))?;
            Ok(())
        });

        // TCP
        tcp.accept_once();
        tcp.poll_once(|batch| {
            parser::parse_and_store_and_write(batch, Some(&store), |line| writer.write_line(line))?;
            Ok(())
        });

        // ── 주기적 하우스키핑 ──────────────────────────────────────────────────────
        if last_housekeep.elapsed().unwrap_or(Duration::from_secs(0)) > Duration::from_millis(200) {
            let _ = writer.rotate_if_needed();
            last_housekeep = SystemTime::now();

            // ── 자동 백업 (용량 쿼터는 handle_backup 내부 ensure_backup_quota로 수행) ──
            if AUTO_BACKUP_ENABLED {
                if last_auto_backup
                    .elapsed()
                    .unwrap_or(Duration::from_secs(0))
                    > Duration::from_secs(AUTO_BACKUP_INTERVAL_SECS)
                {
                    // 파일명: auto/backup_<EPOCH>.tsv  (외부 크레이트 없이 간단히)
                    let epoch_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let path = format!("auto/backup_{}.tsv", epoch_secs);

                    // ConsoleSelect 메서드 래퍼를 이용 → 내부에서 SelectQuery 구성 & ensure_backup_quota 적용
                    if let Err(e) = console_select.handle_backup(&path, AUTO_BACKUP_QUERY_STR) {
                        eprintln!("[auto-backup] skipped or failed: {e}");
                    }

                    last_auto_backup = SystemTime::now();
                }
            }
        }

        // 폴링 슬립
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ───────────────────────────────────────────────────────────────────────────────
// 콘솔 디스패처: 간단한 문자열 파싱으로 ConsoleSelect에 연결
// 사용 예:
//   ShowLogList
//   SelectLog latest limit=100
//   BackupLog logs.tsv
//   BackupLog error_100.tsv "level>=Error latest limit=100"
// ───────────────────────────────────────────────────────────────────────────────
fn dispatch_console_command(cmd_line: &str, console: &Arc<ConsoleSelect>) {
    // 대소문자 구분 완화
    let lower = cmd_line.to_ascii_lowercase();

    if lower == "showloglist" || lower == "show" {
        console.handle_show_list();
        return;
    }

    if lower.starts_with("selectlog") {
        // "SelectLog" 다음의 인자 부분만 추출
        let args = cmd_line.get("SelectLog".len()..).unwrap_or("").trim();
        console.handle_select(args);
        return;
    }

    if lower.starts_with("backuplog") {
        // 형태 1) BackupLog path
        // 형태 2) BackupLog path "쿼리문자열"
        // 첫 공백 기준으로 path / args 분리
        let rest = cmd_line.get("BackupLog".len()..).unwrap_or("").trim();
        if rest.is_empty() {
            eprintln!("Usage: BackupLog <path> [\"query string\"]");
            return;
        }
        // path와 나머지(옵션 쿼리)를 분리
        let mut parts = rest.splitn(2, char::is_whitespace);
        let path = parts.next().unwrap_or("").trim();
        let args = parts.next().map(str::trim).filter(|s| !s.is_empty());

        let args_str = args.unwrap_or("");           // Option<&str> → &str
        if let Err(e) = console.handle_backup(path, args_str) {
            eprintln!("Backup failed: {e}");
        }
        return;
    }

    if lower == "clear" || lower == "clearscreen" || lower == "cls" {
        render_home("SwiftLog", env!("CARGO_PKG_VERSION"), console.store_len(), false);
        return;
    }

    if lower == "home" {
        render_home("SwiftLog", env!("CARGO_PKG_VERSION"), console.store_len(), false);
        return;
    }

    if lower == "help" || lower == "h" {
        println!(
            "Commands:\n\
             - ShowLogList\n\
             - SelectLog <query>\n\
             - BackupLog <path> [\"query\"]\n\
             - ClearScreen (clear/cls)\n\
             - Home\n\
             - Help"
        );
        return;
    }

    // 알 수 없는 명령
    eprintln!("Unknown command: {cmd_line}");
}
