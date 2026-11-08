//! 视频元数据入库（`video_records.db`）。
//!
//! 桌面端通过 `@tauri-apps/plugin-sql` 写同一个库；CLI 直接使用 rusqlite 写入，
//! 表结构与前端迁移（`src/main.rs` 的 `create_video_records_table`）保持一致。
//! 建表语句全部是 `IF NOT EXISTS`，因此 CLI 先建库、应用再启动时迁移仍可正常执行。

use crate::models::VideoInfoItem;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS video_records (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    size INTEGER NOT NULL,
    status TEXT,
    reason TEXT,
    format TEXT,
    duration_ms INTEGER,
    overall_bit_rate TEXT,
    width INTEGER,
    height INTEGER,
    codec TEXT,
    frame_rate TEXT,
    raw_xml TEXT,
    detail_json TEXT,
    scanned_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_vr_format ON video_records(format);
CREATE INDEX IF NOT EXISTS idx_vr_status ON video_records(status);
CREATE INDEX IF NOT EXISTS idx_vr_scanned ON video_records(scanned_at);
"#;

const UPSERT_SQL: &str = r#"
INSERT INTO video_records (
    id, name, path, size, status, reason,
    format, duration_ms, overall_bit_rate,
    width, height, codec, frame_rate,
    raw_xml, detail_json, scanned_at
) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,datetime('now'))
ON CONFLICT(path) DO UPDATE SET
    name=excluded.name, size=excluded.size, status=excluded.status, reason=excluded.reason,
    format=excluded.format, duration_ms=excluded.duration_ms, overall_bit_rate=excluded.overall_bit_rate,
    width=excluded.width, height=excluded.height, codec=excluded.codec, frame_rate=excluded.frame_rate,
    raw_xml=excluded.raw_xml, detail_json=excluded.detail_json, scanned_at=excluded.scanned_at
"#;

/// 打开（必要时创建）视频记录库。WAL + busy_timeout 以便桌面端同时运行时也能写入。
pub fn open(db_path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(db_path)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(conn)
}

/// 批量 UPSERT（按 `path` 去重）。`xml` 为可选的 MediaInfo 原始 XML。
pub fn upsert_items(
    conn: &mut Connection,
    items: &[VideoInfoItem],
    xml: &HashMap<String, String>,
) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(UPSERT_SQL)?;
        for item in items {
            let detail = item.detail.as_ref();
            let general = detail.map(|d| &d.general);
            let video = detail.and_then(|d| d.video_streams.first());
            let detail_json = detail.and_then(|d| serde_json::to_string(d).ok());
            let raw_xml = xml.get(&item.path);

            stmt.execute(params![
                item.path,
                item.name,
                item.path,
                item.size as i64,
                item.status,
                item.reason,
                general.map(|g| g.format.as_str()),
                general.map(|g| g.duration_ms as i64),
                general.map(|g| g.overall_bit_rate.as_str()),
                video.map(|v| v.width as i64),
                video.map(|v| v.height as i64),
                video.map(|v| v.codec.as_str()),
                video.map(|v| v.frame_rate.as_str()),
                raw_xml.map(|s| s.as_str()),
                detail_json,
            ])?;
        }
    }
    tx.commit()?;
    Ok(items.len())
}

/// 当前记录总数。
pub fn record_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM video_records", [], |row| row.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DetailedVideoMeta, DetailedVideoStream, GeneralInfo};

    fn sample_item(path: &str) -> VideoInfoItem {
        VideoInfoItem {
            id: 0,
            name: "a.mkv".into(),
            path: path.into(),
            size: 1024,
            status: "success".into(),
            reason: None,
            detail: Some(DetailedVideoMeta {
                general: GeneralInfo {
                    format: "Matroska".into(),
                    duration_ms: 60_000,
                    overall_bit_rate: "10.0 Mbps".into(),
                    ..Default::default()
                },
                video_streams: vec![DetailedVideoStream {
                    width: 1920,
                    height: 1080,
                    codec: "HEVC".into(),
                    frame_rate: "24".into(),
                    hdr_format: "HDR10".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            duration_sec: Some(60.0),
            width: Some(1920),
            height: Some(1080),
            bitrate_mbps: Some(10.0),
            codec: Some("HEVC".into()),
            frame_rate: Some("24".into()),
        }
    }

    #[test]
    fn upsert_is_idempotent_and_writes_camel_case_detail() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();

        let item = sample_item(r"D:\media\a.mkv");
        let xml = HashMap::new();
        assert_eq!(upsert_items(&mut conn, std::slice::from_ref(&item), &xml).unwrap(), 1);
        assert_eq!(upsert_items(&mut conn, std::slice::from_ref(&item), &xml).unwrap(), 1);
        assert_eq!(record_count(&conn).unwrap(), 1);

        let detail: String = conn
            .query_row(
                "SELECT detail_json FROM video_records WHERE path = ?1",
                [&item.path],
                |row| row.get(0),
            )
            .unwrap();
        assert!(detail.contains("\"videoStreams\""));
        assert!(detail.contains("\"HDR10\""));
    }
}
