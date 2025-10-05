use std::{thread, time::Duration};

/// 터미널 클리어(대부분의 ANSI 터미널 호환)
pub fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}

/// 간단한 타이핑 애니메이션으로 한 줄 출력
fn typewrite_line(s: &str, delay_ms: u64) {
    for ch in s.chars() {
        print!("{ch}");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        thread::sleep(Duration::from_millis(delay_ms));
    }
    println!();
}

/// 홈 화면 렌더링
/// - `app` : 앱 이름 (예: "SwiftLog")
/// - `ver` : 버전 문자열 (예: env!("CARGO_PKG_VERSION"))
/// - `total_logs` : 현재 인메모리 로그 개수
/// - `animated` : true면 간단한 타이핑 연출
pub fn render_home(app: &str, ver: &str, total_logs: usize, animated: bool) {
    clear_screen();

    let logo = r#"
   _____       _  __ _     _             
  / ____|     (_)/ _| |   | |            
 | (___  _   _ _| |_| |__ | | ___   __ _ 
  \___ \| | | | |  _| '_ \| |/ _ \ / _` |
  ____) | |_| | | | | |_) | | (_) | (_| |
 |_____/ \__,_|_|_| |_.__/|_|\___/ \__, |
                                    __/ |
                                   |___/ 
"#;

    if animated {
        for line in logo.lines() {
            typewrite_line(line, 2);
        }
    } else {
        println!("{logo}");
    }

    // 헤더/상태
    println!("┌──────────────────────────────────────────────────────┐");
    println!("│  App      : {app:<40}│");
    println!("│  Version  : {ver:<40}│");
    println!("│  LogStore : {total_logs:<40}│");
    println!("└──────────────────────────────────────────────────────┘");

    // 도움말/명령 요약
    println!();
    println!("Commands:");
    println!("  ShowLogList");
    println!("  SelectLog <query>");
    println!("  BackupLog <path> [\"query\"]");
    println!("  ClearScreen   (또는 clear / cls)");
    println!("  Home          (또는 home)");
    println!("  Help          (또는 help / h)");
    println!();
    println!("예시:");
    println!("  SelectLog latest limit=100");
    println!("  SelectLog level>=Warn code=1000..1999");
    println!("  BackupLog error_100.tsv \"level>=Error latest limit=100\"");
}
