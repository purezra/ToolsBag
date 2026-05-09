import Database from '@tauri-apps/plugin-sql'
import type { VideoInfoItem } from '../types/media'

const DB_URL = 'sqlite:video_records.db'

let dbInstance: Database | null = null

async function getDb(): Promise<Database> {
  if (!dbInstance) {
    dbInstance = await Database.load(DB_URL)
  }
  return dbInstance
}

export interface VideoRecordRow {
  id: string
  name: string
  path: string
  size: number
  status: string | null
  reason: string | null
  format: string | null
  duration_ms: number | null
  overall_bit_rate: string | null
  width: number | null
  height: number | null
  codec: string | null
  frame_rate: string | null
  raw_xml: string | null
  detail_json: string | null
  scanned_at: string | null
}

/**
 * 保存视频记录到数据库（UPSERT，按 path 去重）
 */
export async function saveVideoRecords(
  items: VideoInfoItem[],
  rawXmlMap: Map<string, string>
): Promise<number> {
  const db = await getDb()
  let saved = 0

  await db.execute('BEGIN')
  try {
  for (const item of items) {
    const detail = item.detail
    const general = detail?.general
    const firstVideo = detail?.videoStreams?.[0]

    const id = `${item.path}`
    const rawXml = rawXmlMap.get(item.path) ?? null
    const detailJson = detail ? JSON.stringify(detail) : null

    await db.execute(
      `INSERT INTO video_records (
        id, name, path, size, status, reason,
        format, duration_ms, overall_bit_rate,
        width, height, codec, frame_rate,
        raw_xml, detail_json, scanned_at
      ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,datetime('now'))
      ON CONFLICT(path) DO UPDATE SET
        name=excluded.name, size=excluded.size, status=excluded.status, reason=excluded.reason,
        format=excluded.format, duration_ms=excluded.duration_ms, overall_bit_rate=excluded.overall_bit_rate,
        width=excluded.width, height=excluded.height, codec=excluded.codec, frame_rate=excluded.frame_rate,
        raw_xml=excluded.raw_xml, detail_json=excluded.detail_json, scanned_at=excluded.scanned_at`,
      [
        id,
        item.name,
        item.path,
        item.size,
        item.status,
        item.reason ?? null,
        general?.format ?? null,
        general?.durationMs ?? null,
        general?.overallBitRate ?? null,
        firstVideo?.width ?? null,
        firstVideo?.height ?? null,
        firstVideo?.codec ?? null,
        firstVideo?.frameRate ?? null,
        rawXml,
        detailJson,
      ]
    )
    saved++
  }
  await db.execute('COMMIT')
  } catch (e) {
    await db.execute('ROLLBACK')
    throw e
  }

  return saved
}

/**
 * 加载历史记录（分页）
 */
export async function loadVideoRecords(limit = 50, offset = 0): Promise<VideoRecordRow[]> {
  const db = await getDb()
  return await db.select<VideoRecordRow[]>(
    'SELECT * FROM video_records ORDER BY scanned_at DESC LIMIT $1 OFFSET $2',
    [limit, offset]
  )
}

/**
 * 删除指定 ID 的记录
 */
export async function deleteVideoRecords(ids: string[]): Promise<number> {
  const db = await getDb()
  const placeholders = ids.map((_, i) => `$${i + 1}`).join(',')
  const result = await db.execute(
    `DELETE FROM video_records WHERE id IN (${placeholders})`,
    ids
  )
  return result.rowsAffected
}

/**
 * 获取记录总数
 */
export async function getRecordCount(): Promise<number> {
  const db = await getDb()
  const rows = await db.select<{ cnt: number }[]>(
    'SELECT COUNT(*) as cnt FROM video_records'
  )
  return rows[0]?.cnt ?? 0
}

/**
 * 将数据库行转换为 VideoInfoItem
 */
export function rowToVideoInfoItem(row: VideoRecordRow): VideoInfoItem {
  let detail = undefined
  if (row.detail_json) {
    try {
      detail = JSON.parse(row.detail_json)
    } catch {
      // detail_json 损坏时忽略
    }
  }
  return {
    id: Date.now() + Math.random(),
    name: row.name,
    path: row.path,
    size: row.size,
    status: row.status ?? 'success',
    reason: row.reason ?? undefined,
    detail,
  }
}
