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

// ==================== 数据库统计 ====================

export interface StatBucket {
  label: string
  count: number
  size: number
}

export interface VideoDbStats {
  total: number
  success: number
  failed: number
  totalSize: number
  totalDurationMs: number
  byHdr: StatBucket[]
  byResolution: StatBucket[]
  bySize: StatBucket[]
  byFormat: StatBucket[]
  byCodec: StatBucket[]
  byBitDepth: StatBucket[]
}

const SIZE_BUCKET_ORDER = [
  '< 100 MB',
  '100–500 MB',
  '0.5–1 GB',
  '1–2 GB',
  '2–5 GB',
  '5–10 GB',
  '≥ 10 GB',
  '未知',
]

/** 把 MediaInfo 的 HDR 长字符串归并成几大类，Dolby Vision / HDR10+ 优先匹配。 */
function normalizeHdr(raw: string | null): string {
  if (raw === null) return '未知'
  const value = raw.trim()
  if (!value || value === '-') return 'SDR'
  const lower = value.toLowerCase()
  if (lower.includes('dolby vision') || lower.includes('dvhe') || lower.includes('dvh1')) return 'Dolby Vision'
  if (lower.includes('hdr10+') || lower.includes('hdr10plus') || lower.includes('hdr10 plus')) return 'HDR10+'
  if (lower.includes('hdr10')) return 'HDR10'
  if (lower.includes('hlg')) return 'HLG'
  if (lower.includes('hdr')) return 'HDR（其他）'
  return value
}

function mergeBuckets(label: string, count: number, size: number, target: Map<string, StatBucket>) {
  const existing = target.get(label)
  if (existing) {
    existing.count += count
    existing.size += size
  } else {
    target.set(label, { label, count, size })
  }
}

const sortByCount = (a: StatBucket, b: StatBucket) => b.count - a.count

/**
 * 聚合当前数据库中的全部记录。
 *
 * 分布数据交给 SQLite 做 GROUP BY，避免把 detail_json 全量读进前端。
 */
export async function loadVideoStats(): Promise<VideoDbStats> {
  const db = await getDb()

  const [overall] = await db.select<{
    total: number
    success: number
    failed: number
    total_size: number
    total_duration: number
  }[]>(`
    SELECT
      COUNT(*) AS total,
      COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) AS success,
      COALESCE(SUM(CASE WHEN status IS NULL OR status <> 'success' THEN 1 ELSE 0 END), 0) AS failed,
      COALESCE(SUM(size), 0) AS total_size,
      COALESCE(SUM(duration_ms), 0) AS total_duration
    FROM video_records
  `)

  const [hdrRows, resRows, sizeRows, formatRows, codecRows, depthRows] = await Promise.all([
    db.select<{ raw: string | null; count: number; size: number }[]>(`
      SELECT
        CASE
          WHEN detail_json IS NULL THEN NULL
          WHEN json_valid(detail_json) THEN json_extract(detail_json, '$.videoStreams[0].hdrFormat')
          ELSE NULL
        END AS raw,
        COUNT(*) AS count,
        COALESCE(SUM(size), 0) AS size
      FROM video_records
      GROUP BY raw
    `),
    db.select<{ width: number | null; height: number | null; count: number; size: number }[]>(`
      SELECT width, height, COUNT(*) AS count, COALESCE(SUM(size), 0) AS size
      FROM video_records
      GROUP BY width, height
    `),
    db.select<{ label: string; count: number; size: number }[]>(`
      SELECT
        CASE
          WHEN size < 104857600 THEN '< 100 MB'
          WHEN size < 524288000 THEN '100–500 MB'
          WHEN size < 1073741824 THEN '0.5–1 GB'
          WHEN size < 2147483648 THEN '1–2 GB'
          WHEN size < 5368709120 THEN '2–5 GB'
          WHEN size < 10737418240 THEN '5–10 GB'
          WHEN size > 0 THEN '≥ 10 GB'
          ELSE '未知'
        END AS label,
        COUNT(*) AS count,
        COALESCE(SUM(size), 0) AS size
      FROM video_records
      GROUP BY label
    `),
    db.select<{ label: string | null; count: number; size: number }[]>(`
      SELECT COALESCE(NULLIF(format, ''), '未知') AS label, COUNT(*) AS count, COALESCE(SUM(size), 0) AS size
      FROM video_records
      GROUP BY label
    `),
    db.select<{ label: string | null; count: number; size: number }[]>(`
      SELECT COALESCE(NULLIF(codec, ''), '未知') AS label, COUNT(*) AS count, COALESCE(SUM(size), 0) AS size
      FROM video_records
      GROUP BY label
    `),
    db.select<{ raw: string | null; count: number; size: number }[]>(`
      SELECT
        CASE
          WHEN detail_json IS NULL THEN NULL
          WHEN json_valid(detail_json) THEN json_extract(detail_json, '$.videoStreams[0].bitDepth')
          ELSE NULL
        END AS raw,
        COUNT(*) AS count,
        COALESCE(SUM(size), 0) AS size
      FROM video_records
      GROUP BY raw
    `),
  ])

  const hdrMap = new Map<string, StatBucket>()
  for (const row of hdrRows) {
    mergeBuckets(normalizeHdr(row.raw), row.count, row.size, hdrMap)
  }

  const resolutionMap = new Map<string, StatBucket>()
  for (const row of resRows) {
    const label = row.width && row.height ? `${row.width}×${row.height}` : '未知'
    mergeBuckets(label, row.count, row.size, resolutionMap)
  }

  const sizeMap = new Map<string, StatBucket>()
  for (const row of sizeRows) {
    mergeBuckets(row.label || '未知', row.count, row.size, sizeMap)
  }

  const depthMap = new Map<string, StatBucket>()
  for (const row of depthRows) {
    const raw = row.raw
    const label = raw === null ? '未知' : raw.trim() ? `${raw.trim()} bit` : '未知'
    mergeBuckets(label, row.count, row.size, depthMap)
  }

  const bySize = [...sizeMap.values()].sort(
    (a, b) => SIZE_BUCKET_ORDER.indexOf(a.label) - SIZE_BUCKET_ORDER.indexOf(b.label)
  )

  return {
    total: overall?.total ?? 0,
    success: overall?.success ?? 0,
    failed: overall?.failed ?? 0,
    totalSize: overall?.total_size ?? 0,
    totalDurationMs: overall?.total_duration ?? 0,
    byHdr: [...hdrMap.values()].sort(sortByCount),
    byResolution: [...resolutionMap.values()].sort(sortByCount),
    bySize,
    byFormat: formatRows
      .map((row) => ({ label: row.label || '未知', count: row.count, size: row.size }))
      .sort(sortByCount),
    byCodec: codecRows
      .map((row) => ({ label: row.label || '未知', count: row.count, size: row.size }))
      .sort(sortByCount),
    byBitDepth: [...depthMap.values()].sort(sortByCount),
  }
}

let _idSeq = 0
// ponytail: global counter, crypto.randomUUID() overkill for table row keys
const nextId = () => ++_idSeq

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
    id: nextId(),
    name: row.name,
    path: row.path,
    size: row.size,
    status: row.status ?? 'success',
    reason: row.reason ?? undefined,
    detail,
  }
}
