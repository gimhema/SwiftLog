use std::{fs::File, io::{BufRead, BufReader}};

use eframe::egui;
use egui::{TextEdit, RichText};
use regex::Regex;

// ─────────────────────────────────────────────────────────────────────────────
// 로그 엔트리 & SelectQuery (SwiftLog와 호환되도록 최소 필드 구성)
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LogLevel { Trace=0, Debug=1, Info=2, Warn=3, Error=4 }

impl LogLevel {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "trace" => Some(LogLevel::Trace),
            "debug" => Some(LogLevel::Debug),
            "info"  => Some(LogLevel::Info),
            "warn"  => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub ts_ms: u64,
    pub level: LogLevel,
    pub code: u16,
    pub msg: String,
}

#[derive(Clone, Debug, Default)]
pub struct SelectQuery {
    pub level_min: Option<LogLevel>,
    pub level_max: Option<LogLevel>,
    pub code_range: Option<(u16,u16)>,   // "code=100..200" 또는 단일 "code=123"
    pub since_ms: Option<u64>,
    pub until_ms: Option<u64>,
    pub contains: Option<String>,        // LIKE '%text%'
    pub regex: Option<Regex>,            // 정규식
    pub limit: Option<usize>,
    pub offset: usize,
    pub latest: bool,                    // 최신 우선 정렬
}

// ─────────────────────────────────────────────────────────────────────────────
// 쿼리 파서: "SelectLog"의 인자 문자열과 동일한 문법을 지원
// 예) "latest limit=100 contains=swiftlog level>=info code=1000..1999"
//     "regex=fail.*socket since=1759196530900"
// ─────────────────────────────────────────────────────────────────────────────
fn parse_select_query(s: &str) -> Result<SelectQuery, String> {
    let mut q = SelectQuery::default();
    if s.trim().is_empty() {
        return Ok(q);
    }
    for tok in s.split_whitespace() {
        if tok.eq_ignore_ascii_case("latest") {
            q.latest = true;
            continue;
        }
        if let Some(rest) = tok.strip_prefix("limit=") {
            q.limit = rest.parse::<usize>().ok();
            continue;
        }
        if let Some(rest) = tok.strip_prefix("offset=") {
            q.offset = rest.parse::<usize>().unwrap_or(0);
            continue;
        }
        if let Some(rest) = tok.strip_prefix("level>=") {
            q.level_min = LogLevel::from_str(rest);
            continue;
        }
        if let Some(rest) = tok.strip_prefix("level<=") {
            q.level_max = LogLevel::from_str(rest);
            continue;
        }
        if let Some(rest) = tok.strip_prefix("code=") {
            if let Some((a,b)) = rest.split_once("..") {
                let lo = a.parse::<u16>().map_err(|_| "invalid code range lo")?;
                let hi = b.parse::<u16>().map_err(|_| "invalid code range hi")?;
                q.code_range = Some((lo.min(hi), lo.max(hi)));
            } else {
                let v = rest.parse::<u16>().map_err(|_| "invalid code value")?;
                q.code_range = Some((v,v));
            }
            continue;
        }
        if let Some(rest) = tok.strip_prefix("since=") {
            q.since_ms = rest.parse::<u64>().ok();
            continue;
        }
        if let Some(rest) = tok.strip_prefix("until=") {
            q.until_ms = rest.parse::<u64>().ok();
            continue;
        }
        if let Some(rest) = tok.strip_prefix("contains=") {
            q.contains = Some(rest.to_string());
            continue;
        }
        if let Some(rest) = tok.strip_prefix("regex=") {
            // regex=... 전체를 그대로 패턴으로 사용
            let re = Regex::new(rest).map_err(|e| format!("invalid regex: {e}"))?;
            q.regex = Some(re);
            continue;
        }
        // 알 수 없는 토큰은 무시(원하면 에러 처리)
    }
    Ok(q)
}

// ─────────────────────────────────────────────────────────────────────────────
/* TSV(.log) 로더
   각 줄:  ts_ms \t level_u8 \t code \t message
   예: 1759196530919\t2\t1001\thello swiftlog #0
*/
// ─────────────────────────────────────────────────────────────────────────────
fn load_tsv(path: &str) -> Result<Vec<LogEntry>, String> {
    let f = File::open(path).map_err(|e| format!("open failed: {e}"))?;
    let mut out = Vec::new();
    for (lnum, line) in BufReader::new(f).lines().enumerate() {
        let line = line.map_err(|e| format!("read failed: {e}"))?;
        if line.trim().is_empty() { continue; }
        let mut it = line.splitn(4, '\t');
        let ts = it.next().ok_or("missing ts_ms")?;
        let lev = it.next().ok_or("missing level")?;
        let code = it.next().ok_or("missing code")?;
        let msg = it.next().unwrap_or("");

        let ts_ms: u64 = ts.parse().map_err(|_| format!("line {}: invalid ts_ms", lnum+1))?;
        let level_u8: u8 = lev.parse().map_err(|_| format!("line {}: invalid level", lnum+1))?;
        let level = match level_u8 {
            0 => LogLevel::Trace,
            1 => LogLevel::Debug,
            2 => LogLevel::Info,
            3 => LogLevel::Warn,
            _ => LogLevel::Error,
        };
        let code_u16: u16 = code.parse().map_err(|_| format!("line {}: invalid code", lnum+1))?;

        out.push(LogEntry { ts_ms, level, code: code_u16, msg: msg.to_string() });
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// 필터링/정렬 적용: SelectQuery → 인덱스 목록(self.filtered) 생성
// ─────────────────────────────────────────────────────────────────────────────
fn filter_indices(all: &[LogEntry], q: &SelectQuery) -> Vec<usize> {
    // 1) 조건 만족하는 인덱스 수집
    let mut idxs: Vec<usize> = all.iter().enumerate()
        .filter(|(_, e)| {
            if let Some(min) = q.level_min { if e.level < min { return false; } }
            if let Some(max) = q.level_max { if e.level > max { return false; } }
            if let Some((lo,hi)) = q.code_range { if e.code < lo || e.code > hi { return false; } }
            if let Some(since) = q.since_ms { if e.ts_ms < since { return false; } }
            if let Some(until) = q.until_ms { if e.ts_ms > until { return false; } }
            if let Some(ref c) = q.contains {
                if !e.msg.contains(c) { return false; }
            }
            if let Some(ref r) = q.regex {
                if !r.is_match(&e.msg) { return false; }
            }
            true
        })
        .map(|(i, _)| i)
        .collect();

    // 2) 정렬
    if q.latest {
        idxs.sort_by_key(|&i| std::cmp::Reverse(all[i].ts_ms));
    } else {
        idxs.sort_by_key(|&i| all[i].ts_ms);
    }

    // 3) offset/limit 적용
    let start = q.offset.min(idxs.len());
    let end = if let Some(lim) = q.limit {
        start.saturating_add(lim).min(idxs.len())
    } else {
        idxs.len()
    };
    idxs[start..end].to_vec()
}

// ─────────────────────────────────────────────────────────────────────────────
// GUI 상태
// ─────────────────────────────────────────────────────────────────────────────
struct ViewerApp {
    // 입력 상태
    path_input: String,
    query_input: String,

    // 데이터
    all_logs: Vec<LogEntry>,
    filtered: Vec<usize>, // filtered 인덱스 (원본 all_logs 인덱스)

    // 메시지/상태
    last_error: Option<String>,
    last_info: Option<String>,
}

impl Default for ViewerApp {
    fn default() -> Self {
        Self {
            path_input: String::new(),
            query_input: String::new(),
            all_logs: Vec::new(),
            filtered: Vec::new(),
            last_error: None,
            last_info: None,
        }
    }
}

impl ViewerApp {
    fn apply_query(&mut self) {
        self.last_error = None;
        match parse_select_query(&self.query_input) {
            Ok(q) => {
                self.filtered = filter_indices(&self.all_logs, &q);
                self.last_info = Some(format!("filtered {} / {}", self.filtered.len(), self.all_logs.len()));
            }
            Err(e) => {
                self.last_error = Some(e);
            }
        }
    }

    fn load_file(&mut self, path: &str) {
        self.last_error = None;
        match load_tsv(path) {
            Ok(v) => {
                self.all_logs = v;
                self.filtered = (0..self.all_logs.len()).collect();
                self.last_info = Some(format!("loaded {} rows", self.all_logs.len()));
            }
            Err(e) => {
                self.last_error = Some(e);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// eframe::App 구현
// ─────────────────────────────────────────────────────────────────────────────
impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // UI 내부에서 즉시 self를 mutate 하면 borrow 충돌이 날 수 있으니
        // 의도를 큐에 담아두었다가 UI 블록 밖에서 실행
        let mut to_load: Option<String> = None;
        let mut to_apply_query: bool = false;
        let mut to_reset_query: bool = false;

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Log file:");
                let te = TextEdit::singleline(&mut self.path_input)
                    .hint_text("path/to/your.log (TSV: ts_ms\\tlevel\\tcode\\tmsg)");
                ui.add_sized([400.0, 24.0], te);

                // 파일 탐색기 버튼
                if ui.button("Browse…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("SwiftLog TSV", &["log", "tsv"])
                        .pick_file()
                    {
                        // 선택 즉시 텍스트박스에 반영하고, 로드는 UI 밖에서 실행
                        self.path_input = path.display().to_string();
                        to_load = Some(self.path_input.clone());
                    }
                }

                if ui.button("Load").clicked() {
                    to_load = Some(self.path_input.trim().to_string());
                }
                if ui.button("Clear").clicked() {
                    self.path_input.clear();
                }
            });

            ui.horizontal(|ui| {
                ui.label("SelectLog query:");
                let te = TextEdit::singleline(&mut self.query_input)
                    .hint_text(r#"e.g. latest limit=100 contains=swiftlog level>=info code=1000..1999"#);
                ui.add_sized([600.0, 24.0], te);
                if ui.button("Apply").clicked() { to_apply_query = true; }
                if ui.button("Reset").clicked() { to_reset_query = true; }
            });

            if let Some(ref info) = self.last_info {
                ui.label(RichText::new(info).color(egui::Color32::from_rgb(100, 200, 100)));
            }
            if let Some(ref err) = self.last_error {
                ui.label(RichText::new(err).color(egui::Color32::RED));
            }
        });

        // UI 밖에서 안전하게 상태 변경
        if let Some(path) = to_load { self.load_file(&path); }
        if to_apply_query { self.apply_query(); }
        if to_reset_query {
            self.query_input.clear();
            self.filtered = (0..self.all_logs.len()).collect();
            self.last_info = Some(format!("filtered {} / {}", self.filtered.len(), self.all_logs.len()));
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // 헤더
            ui.separator();
            ui.horizontal(|ui| {
                ui.monospace(format!("{:<23} {:<7} {:<6} {}", "timestamp", "level", "code", "message"));
            });
            ui.separator();

            // 테이블 (간단 렌더)
            let text_height = egui::TextStyle::Body.resolve(ui.style()).size + 4.0;
            egui::ScrollArea::vertical().show_rows(ui, text_height, self.filtered.len(), |ui, row_range| {
                for row in row_range {
                    let idx = self.filtered[row];
                    let e = &self.all_logs[idx];

                    let level_str = match e.level {
                        LogLevel::Trace => "Trace",
                        LogLevel::Debug => "Debug",
                        LogLevel::Info  => "Info",
                        LogLevel::Warn  => "Warn",
                        LogLevel::Error => "Error",
                    };

                    // (선택) ts_ms → 로컬 시각 문자열 (feature=chrono)
                    #[cfg(feature = "chrono")]
                    let ts_fmt = {
                        use chrono::{Local, TimeZone};
                        let dt = Local.timestamp_millis_opt(e.ts_ms as i64).single()
                            .unwrap_or_else(|| Local.timestamp_opt(0,0).unwrap());
                        dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
                    };

                    // feature 없이 기본은 ts_ms 그대로
                    #[cfg(not(feature = "chrono"))]
                    let ts_fmt = e.ts_ms.to_string();

                    ui.monospace(format!("{:<23} {:<7} {:<6} {}", ts_fmt, level_str, e.code, e.msg));
                }
            });
        });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // 기본 배경(어두운 회색)
        egui::Rgba::from_rgba_unmultiplied(0.10, 0.10, 0.11, 1.0).to_array()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 엔트리 포인트
// ─────────────────────────────────────────────────────────────────────────────
fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([700.0, 400.0])
            .with_title("swl-viewer"),
        ..Default::default()
    };
    eframe::run_native(
        "swl-viewer",
        native_options,
        Box::new(|_cc| Box::new(ViewerApp::default())),
    )
}
