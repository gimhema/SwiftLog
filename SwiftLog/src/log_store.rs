// src/log_store.rs
use std::collections::VecDeque;
use std::sync::{Arc, RwLock, atomic::{AtomicU64, Ordering}};
use crate::log_domain::{Log, LogLevel};

#[derive(Clone, Debug, Default)]
pub struct SelectQuery {
    pub level_min: Option<LogLevel>,
    pub level_max: Option<LogLevel>,
    pub code_in: Option<Vec<u16>>,           // 일부 코드만
    pub code_range: Option<(u16,u16)>,       // 구간
    pub since_ms: Option<u64>,
    pub until_ms: Option<u64>,
    pub contains: Option<String>,            // 부분 문자열
    pub regex: Option<regex::Regex>,         // 고급 패턴 (선택)
    pub limit: Option<usize>,
    pub offset: usize,
    pub latest: bool,                        // true면 ID/시간 내림차순 반환
}

pub struct LogStore {
    // 샤드 수는 코어 수에 맞춰 조절 가능
    shards: Vec<RwLock<VecDeque<Arc<Log>>>>,
    cap_per_shard: usize,
    seq: AtomicU64,
}

impl LogStore {
    pub fn with_capacity(total_cap: usize, shards: usize) -> Self {
        let cap = (total_cap.max(shards)) / shards;
        let mut v = Vec::with_capacity(shards);
        for _ in 0..shards {
            v.push(RwLock::new(VecDeque::with_capacity(cap)));
        }
        Self { shards: v, cap_per_shard: cap, seq: AtomicU64::new(1) }
    }

    #[inline]
    fn pick_shard(&self, id: u64) -> usize { (id as usize) % self.shards.len() }

    pub fn append(&self, mut log: Log) -> Arc<Log> {
        let id = self.seq.fetch_add(1, Ordering::Relaxed);
        log.id = id;
        let shard_idx = self.pick_shard(id);
        let arc = Arc::new(log);
        let mut q = self.shards[shard_idx].write().unwrap();
        if q.len() >= self.cap_per_shard { q.pop_front(); } // ring
        q.push_back(arc.clone());
        arc
    }

    pub fn clear(&self) {
        for s in &self.shards {
            s.write().unwrap().clear();
        }
    }

    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.read().unwrap().len()).sum()
    }

    pub fn select(&self, q: &SelectQuery) -> Vec<Arc<Log>> {
        let mut out = Vec::new();
        // 샤드 병합 스캔 (간단/안전: 먼저 모으고 정렬 → 필터)
        for s in &self.shards {
            let guard = s.read().unwrap();
            out.extend(guard.iter().cloned());
        }
        // 정렬: 최신 우선이면 id 내림차순, 아니면 오름차순
        if q.latest {
            out.sort_by(|a,b| b.id.cmp(&a.id));
        } else {
            out.sort_by(|a,b| a.id.cmp(&b.id));
        }
        // 필터
        out.retain(|e| {
            if let Some(min) = q.level_min { if e.level < min { return false; } }
            if let Some(max) = q.level_max { if e.level > max { return false; } }
            if let Some((lo,hi)) = q.code_range { if e.code < lo || e.code > hi { return false; } }
            if let Some(ref codes) = q.code_in { if !codes.contains(&e.code) { return false; } }
            if let Some(since) = q.since_ms { if e.ts_ms < since { return false; } }
            if let Some(until) = q.until_ms { if e.ts_ms > until { return false; } }
            if let Some(ref sub) = q.contains { if !e.msg.contains(sub) { return false; } }
            if let Some(ref re) = q.regex { if !re.is_match(&e.msg) { return false; } }
            true
        });
        // 페이징
        let start = q.offset.min(out.len());
        let mut out = out.split_off(start);
        if let Some(limit) = q.limit { out.truncate(limit); }
        out
    }
}
