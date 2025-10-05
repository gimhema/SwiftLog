use std::sync::Arc;
use shell_words;
use regex::Regex;
use std::io::Write;
use crate::log_store::{LogStore, SelectQuery};
use crate::log_domain::LogLevel;

pub struct ConsoleSelect { store: Arc<LogStore> }
impl ConsoleSelect {
    pub fn new(store: Arc<LogStore>) -> Self { Self { store } }

    pub fn handle_show_list(&self) {
        let q = SelectQuery { latest: true, limit: Some(50), ..Default::default() };
        self.print(&q);
    }
    pub fn handle_select(&self, args: &str) {
        let q = self.parse(args).unwrap_or_default();
        self.print(&q);
    }
    pub fn handle_backup(&self, path: &str, args: Option<&str>) {
        let q = args.map(|s| self.parse(s).unwrap_or_default()).unwrap_or_default();
        let rows = self.store.select(&q);
        let mut f = std::fs::File::create(path).expect("create backup");
        for e in rows {
            let msg = e.msg.replace('\n', "\\n").replace('\t', "\\t");
            let _ = writeln!(f, "{}\t{}\t{}\t{}", e.ts_ms, e.level as u8, e.code, msg);
        }
        println!("backup ok -> {}", path);
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
}
fn parse_level(s: &str) -> Result<LogLevel,String>{
    match s.to_ascii_lowercase().as_str(){
        "trace"=>Ok(LogLevel::Trace),"debug"=>Ok(LogLevel::Debug),"info"=>Ok(LogLevel::Info),
        "warn"=>Ok(LogLevel::Warn),"error"=>Ok(LogLevel::Error),_=>Err("invalid level".into())
    }
}
fn truncate(s:&str,n:usize)->String{ if s.len()<=n{s.to_string()}else{format!("{}…",&s[..n])} }
