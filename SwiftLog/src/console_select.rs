use std::sync::Arc;
use shell_words;
use regex::Regex;
use std::fs::File;
use std::path::Path;
use std::io::{BufWriter, Write};
use crate::log_store::{LogStore, SelectQuery};
use crate::log_domain::LogLevel;


use crate::backup_quota::{ensure_backup_quota, QuotaConfig};
use crate::proto::{BACKUP_MAX_DIR_BYTES, BACKUP_MIN_FS_FREE_BYTES};

pub struct ConsoleSelect { store: Arc<LogStore> }
impl ConsoleSelect {
    pub fn new(store: Arc<LogStore>) -> Self { Self { store } }

    pub fn store_len(&self) -> usize { self.store.len() } 

    pub fn handle_show_list(&self) {
        let q = SelectQuery { latest: true, limit: Some(50), ..Default::default() };
        self.print(&q);
    }
    pub fn handle_select(&self, args: &str) {
        let q = self.parse(args).unwrap_or_default();
        self.print(&q);
    }
    
    fn print(&self, q: &SelectQuery) {
        let rows = self.store.select(q);
        println!("{:>6} | {:>13} | {:>5} | {:>5} | {}", "ID","TS(ms)","LVL","CODE","MESSAGE");
        println!("{}", "-".repeat(80));
        for e in rows {
            println!("{:>6} | {:>13} | {:>5} | {:>5} | {}",
                e.id, e.ts_ms, format!("{:?}", e.level), e.code, truncate(&e.msg, 200));
        }
    }

    fn parse(&self, s: &str) -> Result<SelectQuery, String> {
        let mut q = SelectQuery::default();
        for tok in shell_words::split(s).map_err(|e| e.to_string())? {
            if tok.eq_ignore_ascii_case("latest") { q.latest = true; continue; }
            if let Some((k,v)) = tok.split_once('=') {
                match k.to_ascii_lowercase().as_str() {
                    "limit"  => q.limit = Some(v.parse::<usize>().map_err(|_| "limit")?),
                    "offset" => q.offset = v.parse::<usize>().map_err(|_| "offset")?,
                    "since"  => q.since_ms = Some(v.parse::<u64>().map_err(|_| "since")?),
                    "until"  => q.until_ms = Some(v.parse::<u64>().map_err(|_| "until")?),
                    "contains" => q.contains = Some(v.trim_matches('"').to_string()),
                    "regex"  => q.regex = Some(Regex::new(v.trim_matches('"')).map_err(|e| e.to_string())?),
                    "code_in" => {
                        q.code_in = Some(v.split(',').filter_map(|t| t.parse::<u16>().ok()).collect());
                    }
                    k if k=="level>=" => q.level_min = Some(parse_level(v)?),
                    k if k=="level<=" => q.level_max = Some(parse_level(v)?),
                    "code" if v.contains("..") => {
                        let (a,b)=v.split_once("..").ok_or("code range")?;
                        q.code_range=Some((a.parse().map_err(|_|"code lo")?, b.parse().map_err(|_|"code hi")?));
                    }
                    other => return Err(format!("unknown key: {}", other)),
                }
            } else if tok.starts_with("level>=") { q.level_min=Some(parse_level(&tok[7..])?);
            } else if tok.starts_with("level<=") { q.level_max=Some(parse_level(&tok[7..])?); }
        }
        Ok(q)
    }

    pub fn handle_backup(&self, output_path: &str, args: &str) -> Result<(), String> {
        // args → SelectQuery 변환. 프로젝트에 실제 파서가 있다면 그걸 쓰세요.
        // (예: SelectQuery::from_shell_words(args)?). 없으면 기본값으로.
        let query = if args.trim().is_empty() {
            SelectQuery::default()
        } else {
            // TODO: 실제 파서가 있으면 아래 줄 교체
            SelectQuery::default()
        };

        // self가 LogStore를 보관한다고 가정(Arc<LogStore>면 &*로 빌리면 됩니다)
        let store: &crate::log_store::LogStore = &self.store;
        super::console_select::handle_backup(store, output_path, &query)
    }

}
fn parse_level(s: &str) -> Result<LogLevel,String>{
    match s.to_ascii_lowercase().as_str(){
        "trace"=>Ok(LogLevel::Trace),"debug"=>Ok(LogLevel::Debug),"info"=>Ok(LogLevel::Info),
        "warn"=>Ok(LogLevel::Warn),"error"=>Ok(LogLevel::Error),_=>Err("invalid level".into())
    }
}
fn truncate(s:&str,n:usize)->String{ if s.len()<=n{s.to_string()}else{format!("{}…",&s[..n])} }


    pub fn handle_backup(
    store: &LogStore,
    output_path: &str,
    query: &SelectQuery,
) -> Result<(), String> {
    let out_path = Path::new(output_path);
    let parent = out_path.parent().ok_or_else(|| "invalid output path".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("create dir failed: {e}"))?;

    // 1) 예상 크기 계산
    let results = store.select(query);
    let mut expected_bytes: u64 = 0;
    for log in &results {
        let msg = escape_tsv(&log.msg);
        expected_bytes = expected_bytes
            .saturating_add(num_len_u64(log.ts_ms) as u64)
            .saturating_add(1) // \t
            .saturating_add(3) // level 자리수 여유
            .saturating_add(1) // \t
            .saturating_add(num_len_u16(log.code) as u64)
            .saturating_add(1) // \t
            .saturating_add(msg.len() as u64)
            .saturating_add(1); // \n
    }

    // 2) 쿼터 체크
    let cfg = QuotaConfig {
        max_dir_bytes: BACKUP_MAX_DIR_BYTES,
        min_fs_free_bytes: BACKUP_MIN_FS_FREE_BYTES,
    };
    ensure_backup_quota(parent, expected_bytes, cfg)
        .map_err(|e| format!("{e:?}"))?;

    // 3) 실제 쓰기
    let file = File::create(&out_path)
        .map_err(|e| format!("create file failed: {e}"))?;
    let mut w = BufWriter::new(file);
    for log in &results {
        let line = format!(
            "{}\t{}\t{}\t{}\n",
            log.ts_ms as u64,
            log.level as u8,
            log.code,
            escape_tsv(&log.msg),
        );
        w.write_all(line.as_bytes())
            .map_err(|e| format!("write failed: {e}"))?;
    }
    w.flush().map_err(|e| format!("flush failed: {e}"))?;
    Ok(())
}


fn escape_tsv(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn num_len_u64(mut x: u64) -> usize {
    if x == 0 { return 1; }
    let mut n = 0;
    while x > 0 { x /= 10; n += 1; }
    n
}
fn num_len_u16(x: u16) -> usize { num_len_u64(x as u64) }