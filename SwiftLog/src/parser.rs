use crate::proto::{MAGIC, VERSION};
use std::convert::TryInto;
use std::io;

use std::sync::Arc;
use crate::log_store::LogStore;
use crate::log_domain::{Log, LogLevel};

/// (하위 호환) 기존 시그니처 유지:
/// 배치 바이트 → 각 로그 라인을 TSV로 만들어 writer(sink)에 전달.
/// 인메모리 저장은 하지 않음. (store = None)
pub fn parse_and_write(batch: &[u8], mut sink: impl FnMut(&[u8]) -> io::Result<()>) {
    let _ = parse_and_store_and_write(batch, None, &mut sink);
}

/// 새 함수:
/// 배치 바이트 → TSV 라인 writer(sink)에 전달 + (옵션) 인메모리 LogStore에 append.
/// `store`가 Some이면 인메모리 적재, None이면 파일 쓰기만 수행.
pub fn parse_and_store_and_write(
    batch: &[u8],
    store: Option<&Arc<LogStore>>,
    mut sink: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    // 헤더 검사
    if batch.len() < 8 { return Ok(()); }
    if &batch[0..4] != &MAGIC.to_le_bytes() { return Ok(()); }
    if &batch[4..6] != &VERSION.to_le_bytes() { return Ok(()); }
    let count = u16::from_le_bytes([batch[6], batch[7]]) as usize;

    let mut p = 8usize;
    for _ in 0..count {
        // 남은 버퍼가 엔트리 고정 영역(8+1+2+2)보다 작으면 중단
        if p + 8 + 1 + 2 + 2 > batch.len() { break; }

        let ts_ms = u64::from_le_bytes(batch[p..p+8].try_into().unwrap()); p += 8;
        let level_u8 = batch[p]; p += 1;
        let code = u16::from_le_bytes(batch[p..p+2].try_into().unwrap()); p += 2;
        let len  = u16::from_le_bytes(batch[p..p+2].try_into().unwrap()) as usize; p += 2;

        if p + len > batch.len() { break; }
        let msg_bytes = &batch[p..p+len]; p += len;

        // --- (1) 파일용 TSV 라인 구성 후 sink에 기록 --------------------------------
        // 포맷: ts \t level \t code \t message
        // (필요시 이스케이프를 추가해도 됨: 탭/개행 등)
        let mut line = Vec::with_capacity(64 + len);
        line.extend_from_slice(ts_ms.to_string().as_bytes());
        line.push(b'\t');
        line.extend_from_slice(level_u8.to_string().as_bytes());
        line.push(b'\t');
        line.extend_from_slice(code.to_string().as_bytes());
        line.push(b'\t');
        line.extend_from_slice(msg_bytes);

        sink(&line)?; // 파일/파이프 등에 기록

        // --- (2) 인메모리 저장소에 append (옵션) -----------------------------------
        if let Some(store) = store {
            on_parsed_entry(ts_ms, level_u8, code, msg_bytes, store);
        }
    }
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
