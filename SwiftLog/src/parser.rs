use crate::proto::{MAGIC, VERSION};
use std::convert::TryInto;
use std::io;

use std::sync::Arc;
use crate::log_store::LogStore;
use crate::log_domain::{Log, LogLevel};

use crate::logger::{self, *};

pub fn parse_and_write(batch: &[u8], mut sink: impl FnMut(&[u8]) -> io::Result<()>) {
    let _ = parse_and_store_and_write(batch, None, &mut sink);
}

pub fn parse_and_store_and_write(
    batch: &[u8],
    store: Option<&Arc<LogStore>>,
    mut sink: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    // 헤더 검사: C++는 magic(4) + version(u32, 4) 를 씁니다.
    logger::log_to_file("parse_and_store_and_write Step 1");
    if batch.len() < 8 { return Ok(()); }
    logger::log_to_file("parse_and_store_and_write Step 2");
    if &batch[0..4] != &MAGIC.to_le_bytes() { return Ok(()); }
    logger::log_to_file("parse_and_store_and_write Step 3");
    if &batch[4..8] != &VERSION.to_le_bytes() { return Ok(()); }

    logger::log_to_file("parse_and_store_and_write Step 4");


    let mut p = 8usize;
    loop {
        logger::log_to_file("parse_and_store_and_write Step 5");
        // 각 레코드의 최소 고정 영역: id(8) + ts_ms(8) + level(1) + code(2) + len(2)
        const MIN_RECORD_FIXED: usize = 8 + 8 + 1 + 2 + 2;
        if p + MIN_RECORD_FIXED > batch.len() { break; }

        logger::log_to_file("parse_and_store_and_write Step 6");

        // id (u64) -- 현재는 사용하지 않지만 포맷상 먼저 옴
        let _id = u64::from_le_bytes(batch[p..p+8].try_into().unwrap());
        p += 8;

        // ts_ms (u64)
        let ts_ms = u64::from_le_bytes(batch[p..p+8].try_into().unwrap());
        p += 8;

        // level (u8)
        let level_u8 = batch[p];
        p += 1;

        // code (u16, LE)
        let code = u16::from_le_bytes(batch[p..p+2].try_into().unwrap());
        p += 2;

        // msg_len (u16, LE)
        let len = u16::from_le_bytes(batch[p..p+2].try_into().unwrap()) as usize;
        p += 2;

        // 메시지 바이트가 충분히 남았는지 체크
        if p + len > batch.len() { break; }
        logger::log_to_file("parse_and_store_and_write Step 7");        
        let msg_bytes = &batch[p..p+len];
        p += len;

        // --- (1) 파일용 TSV 라인 구성 후 sink에 기록 ------------------------------
        let mut line = Vec::with_capacity(64 + len);
        line.extend_from_slice(ts_ms.to_string().as_bytes());
        line.push(b'\t');
        line.extend_from_slice(level_u8.to_string().as_bytes());
        line.push(b'\t');
        line.extend_from_slice(code.to_string().as_bytes());
        line.push(b'\t');
        line.extend_from_slice(msg_bytes);

        sink(&line)?; // 파일/파이프 등에 기록

        // --- (2) 인메모리 저장소에 append (옵션) --------------------------------
        if let Some(store) = store {
            on_parsed_entry(ts_ms, level_u8, code, msg_bytes, store);
        }
    }
    logger::log_to_file("parse_and_store_and_write Step End");
    Ok(())
}

// 내부 유틸: 파싱된 엔트리 1건을 LogStore에 적재
fn on_parsed_entry(
    ts_ms: u64,
    level_u8: u8,
    code: u16,
    msg_bytes: &[u8],
    store: &Arc<LogStore>,
) {
    let level = match level_u8 {
        0 => LogLevel::Trace,
        1 => LogLevel::Debug,
        2 => LogLevel::Info,
        3 => LogLevel::Warn,
        _ => LogLevel::Error,
    };
    let msg = String::from_utf8_lossy(msg_bytes).into_owned();
    let log = Log::new_unassigned(ts_ms, level, code, msg);
    if log.validate().is_ok() {
        store.append(log);
    }
}
