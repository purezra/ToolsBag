export type DurationFormat = 'hms' | 'clock' | 'minutes'

export const formatBytes = (value: number): string => {
  if (!value || Number.isNaN(value)) return '0 B'
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`
  if (value < 1024 * 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`
  if (value < 1024 * 1024 * 1024 * 1024) return `${(value / 1024 / 1024 / 1024).toFixed(2)} GB`
  return `${(value / 1024 / 1024 / 1024 / 1024).toFixed(2)} TB`
}

export const formatThroughput = (bytesPerSec: number): string => {
  if (!bytesPerSec) return '0 B/s'
  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s']
  const idx = Math.min(units.length - 1, Math.floor(Math.log(bytesPerSec) / Math.log(1024)))
  const num = bytesPerSec / Math.pow(1024, idx)
  return `${num.toFixed(num >= 10 ? 1 : 2)} ${units[idx]}`
}

export const formatDuration = (seconds: number | undefined, mode: DurationFormat = 'hms'): string => {
  const val = seconds ?? 0
  if (!Number.isFinite(val)) return '-'
  const h = Math.floor(val / 3600)
  const m = Math.floor((val % 3600) / 60)
  const s = Math.floor(val % 60)
  if (mode === 'clock') {
    if (h > 0) {
      return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
    }
    return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
  }
  if (mode === 'minutes') {
    const minutes = val / 60
    return `${minutes.toFixed(4)}min`
  }
  const parts: string[] = []
  if (h) parts.push(`${h}h`)
  if (m) parts.push(`${m}m`)
  parts.push(`${s}s`)
  return parts.join('')
}
