/**
 * Shared media utility functions extracted from composables
 * to avoid duplication between useVideoInfoHealth and useVideoInfoColumns.
 */

/** Parse a bitrate string to Mbps numeric value. Supports "10 Mbps" / "562 kb/s" / "12500000 bps" etc. */
export const parseBitRateMbps = (s: string | undefined): number => {
  if (!s) return 0
  const lower = s.toLowerCase()
  const m = s.match(/[\d]+(?:\.[\d]+)?/)
  const n = m ? parseFloat(m[0]) : 0
  if (n <= 0) return 0
  if (lower.includes('mb')) return n
  if (lower.includes('kb') || lower.includes('kbit')) return n / 1000
  if (n >= 100_000) return n / 1_000_000
  if (n >= 1000) return n / 1000
  return n
}

/** Compare two numbers respecting ascending/descending sort order. */
export const compareByOrder = (a: number, b: number, order: 'ascending' | 'descending' | null): number => {
  if (!order) return 0
  return order === 'ascending' ? a - b : b - a
}

export type DurationFilterMode = 'all' | 'at-most' | 'at-least' | 'between'
export type DurationUnit = 'second' | 'minute' | 'hour'

export type VideoFilterState = {
  formats: string[]
  durationMode: DurationFilterMode
  durationMin: number
  durationMax: number
  durationUnit: DurationUnit
  recentDays: number | null
}

type FilterableVideo = {
  name: string
  durationSec?: number | null
  arrivalTimeMs?: number | null
}

export type VideoFilterDecision = 'match' | 'no-match' | 'missing'

export const VIDEO_EXTENSIONS = ['mp4', 'mov', 'mkv', 'avi', 'm4v', 'wmv', 'flv', 'webm', 'ts', 'mts', 'm2ts']
export const IMAGE_EXTENSIONS = ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'tiff', 'webp', 'heic', 'heif']

export type MediaFormatCount = { ext: string; count: number }

export const splitMediaFormatCounts = (formatCounts: MediaFormatCount[]) => {
  const sorted = [...formatCounts].sort((a, b) => b.count - a.count || a.ext.localeCompare(b.ext))
  return {
    videoFormats: sorted.filter(({ ext }) => VIDEO_EXTENSIONS.includes(ext)),
    imageFormats: sorted.filter(({ ext }) => IMAGE_EXTENSIONS.includes(ext)),
  }
}

export const paginateRows = <T>(rows: T[], requestedPage: number, pageSize: number) => {
  const safePageSize = Math.max(1, Math.floor(pageSize))
  const pageCount = Math.max(1, Math.ceil(rows.length / safePageSize))
  const page = Math.min(pageCount, Math.max(1, Math.floor(requestedPage) || 1))
  const start = (page - 1) * safePageSize
  return {
    page,
    pageCount,
    rows: rows.slice(start, start + safePageSize),
  }
}

const durationMultiplier: Record<DurationUnit, number> = {
  second: 1,
  minute: 60,
  hour: 3600,
}

export const evaluateVideoFilter = (
  video: FilterableVideo,
  filters: VideoFilterState,
  nowMs = Date.now(),
): VideoFilterDecision => {
  const extension = video.name.includes('.') ? video.name.slice(video.name.lastIndexOf('.') + 1).toLowerCase() : ''
  if (filters.formats.length && !filters.formats.includes(extension)) return 'no-match'

  if (filters.durationMode !== 'all') {
    if (video.durationSec == null) return 'missing'
    const multiplier = durationMultiplier[filters.durationUnit]
    const min = filters.durationMin * multiplier
    const max = filters.durationMax * multiplier
    if (filters.durationMode === 'at-most' && video.durationSec > max) return 'no-match'
    if (filters.durationMode === 'at-least' && video.durationSec < min) return 'no-match'
    if (filters.durationMode === 'between' && (video.durationSec < min || video.durationSec > max)) return 'no-match'
  }

  if (filters.recentDays !== null) {
    if (video.arrivalTimeMs == null) return 'missing'
    const cutoff = nowMs - filters.recentDays * 24 * 60 * 60 * 1000
    if (video.arrivalTimeMs < cutoff) return 'no-match'
  }

  return 'match'
}
