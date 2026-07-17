//! 媒体探针结果 SQLite 缓存
//!
//! 路径 + mtime 不变时复用提取结果，避免重复调用 MediaInfo。

use once_cell::sync::OnceCell;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

/// 全局缓存连接（惰式初始化，Mutex 保护线程安全）
static CACHE: OnceCell<Mutex<Connection>> = OnceCell::new();
static CACHE_WRITES: AtomicUsize = AtomicUsize::new(0);

/// 初始化缓存数据库。可显式指定 db 路径；若已初始化则跳过。
pub fn init_cache(db_path: &Path) {
    if CACHE.get().is_some() {
        return;
    }
    let _ = CACHE.get_or_init(|| {
        let conn = match Connection::open(db_path) {
            Ok(c) => c,
            Err(e) => {
                log::error!("media_cache: 无法打开数据库 {:?}: {}", db_path, e);
                Connection::open_in_memory().expect("media_cache: 内存数据库也无法创建")
            }
        };
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS media_cache (
                 path  TEXT PRIMARY KEY,
                 mtime_ms INTEGER NOT NULL,
                 schema_ver INTEGER NOT NULL DEFAULT 1,
                 data TEXT NOT NULL
             );",
        )
        .unwrap_or_else(|e| log::error!("media_cache: 建表失败: {}", e));
        Mutex::new(conn)
    });
}

/// 缓存 schema 版本：修复 duration 秒数丢失等问题后递增，使旧缓存自动失效
const CACHE_SCHEMA_VERSION: i64 = 4;

/// 查询缓存：路径 + mtime + schema版本 匹配则返回缓存的 JSON 值
pub fn get_cached_item(path: &str, mtime_ms: u64) -> Option<serde_json::Value> {
    let conn = CACHE.get()?;
    let guard = conn.lock().unwrap();
    let mut stmt = guard
        .prepare(
            "SELECT data FROM media_cache WHERE path = ?1 AND mtime_ms = ?2 AND schema_ver = ?3",
        )
        .ok()?;
    let data: String = stmt
        .query_row(
            rusqlite::params![path, mtime_ms as i64, CACHE_SCHEMA_VERSION],
            |row| row.get(0),
        )
        .ok()?;
    serde_json::from_str(&data).ok()
}

/// 清理过期缓存，保留最近的 max_entries 条，删除其余
pub fn cleanup_cache(max_entries: usize) {
    let Some(conn) = CACHE.get() else {
        return;
    };
    let guard = conn.lock().unwrap();
    let count: i64 = guard
        .query_row("SELECT COUNT(*) FROM media_cache", [], |row| row.get(0))
        .unwrap_or(0);
    if count as usize > max_entries {
        if let Err(e) = guard.execute(
            "DELETE FROM media_cache WHERE rowid NOT IN (SELECT rowid FROM media_cache ORDER BY rowid DESC LIMIT ?1)",
            rusqlite::params![max_entries as i64],
        ) {
            log::error!("media_cache: 清理失败: {}", e);
        }
        log::info!("media_cache: 清理完成，{} -> {}", count, max_entries);
    }
}

/// 写入/更新缓存
pub fn set_cached_item(path: &str, mtime_ms: u64, data: &serde_json::Value) {
    let Some(conn) = CACHE.get() else {
        log::error!("media_cache: 未初始化，写入跳过");
        return;
    };
    let guard = conn.lock().unwrap();
    let json = match serde_json::to_string(data) {
        Ok(j) => j,
        Err(e) => {
            log::error!("media_cache: 序列化失败: {}", e);
            return;
        }
    };
    if let Err(e) = guard.execute(
        "INSERT OR REPLACE INTO media_cache (path, mtime_ms, schema_ver, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![path, mtime_ms as i64, CACHE_SCHEMA_VERSION, json],
    ) {
        log::error!("media_cache: 写入失败: {}", e);
    }
    drop(guard);
    // 清理无需跟随每个并行探针执行；每 100 次写入检查一次即可。
    if CACHE_WRITES.fetch_add(1, Ordering::Relaxed) % 100 == 99 {
        cleanup_cache(5000);
    }
}
