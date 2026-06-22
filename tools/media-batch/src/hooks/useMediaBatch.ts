import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { listen } from '@tauri-apps/api/event'
import { importMedia, getMediaInfoStatus } from '../api/media-batch'
import { openParentDir, pathExists, renamePath } from '@core/api/common'
import { formatBytes, formatDuration, type DurationFormat } from '@core/utils/format'
import { hasTauriRuntime } from '@core/utils/tauri'
import { useFileSelect } from '@core/hooks/useFileSelect'
import { useSettings } from '@core/hooks/useSettings'
import type { ImageRow, MediaKind, MediaInfoStatus, RenameField, RenameSafetySummary, VideoRow } from '../types/media'

const VIDEO_EXTS = ['mp4', 'mov', 'mkv', 'avi', 'm4v', 'wmv', 'flv', 'webm', 'ts', 'mts', 'm2ts']
const IMAGE_EXTS = ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'tiff', 'webp', 'heic', 'heif']

type ImportKind = 'file' | 'folder' | 'clipboard'
type OrganizePresetKey = 'short-video' | 'archive-video' | 'photo-exif'

type LiveImportItem = {
  id: string
  name: string
  path: string
  source: ImportKind
  status: 'pending' | 'success' | 'failed'
}

type RenameRow = (VideoRow | ImageRow) & { previewName: string; order: number }

type RenamePlanItem = {
  row: RenameRow
  sourcePath: string
  sourceName: string
  targetPath: string
  targetName: string
  unchanged: boolean
  warnings: string[]
}

type UndoRenameItem = {
  originalPath: string
  originalName: string
  currentPath: string
  currentName: string
  kind: MediaKind
}

// 配置常量
const LIVE_IMPORT_SNAPSHOT_LIMIT = 80
const LIVE_IMPORT_DISPLAY_LIMIT = 120

export const useMediaBatch = () => {
  const { pick } = useFileSelect()
  const { t } = useSettings()

  // MediaInfo 状态
  const mediaInfoStatus = ref<MediaInfoStatus>({ available: false })

  const fileTypeTab = ref<MediaKind>('video')
  const batchSize = ref(20)
  const allowAutoRefresh = ref(true)
  const recursive = ref(false)
  const importing = ref(false)
  const importSummary = ref('')
  const importProgress = reactive({
    active: false,
    batch: 0,
    totalBatches: 0,
    totalItems: 0,
    percent: 0
  })
  const failedItems = ref<(VideoRow | ImageRow)[]>([])
  const failedReasonStats = computed(() => {
    const map = new Map<string, number>()
    failedItems.value.forEach((item) => {
      const reason = (item.reason || '未知原因').trim() || '未知原因'
      map.set(reason, (map.get(reason) || 0) + 1)
    })
    return Array.from(map.entries()).map(([reason, count]) => ({ reason, count })).sort((a, b) => b.count - a.count)
  })
  const showFailedDialog = ref(false)

  const renameFieldsVideo = reactive<RenameField[]>([
    { key: 'seq', label: '序号', enabled: true },
    { key: 'duration', label: '时长', enabled: true },
    { key: 'custom', label: '自定义文本', enabled: true },
    { key: 'resolution', label: '分辨率', enabled: false },
    { key: 'bitrate', label: '码率', enabled: false },
    { key: 'filename', label: '原始文件名', enabled: false },
    { key: 'size', label: '文件大小', enabled: false }
  ])

  const renameFieldsImage = reactive<RenameField[]>([
    { key: 'seq', label: '序号', enabled: true },
    { key: 'resolution', label: '分辨率', enabled: true },
    { key: 'custom', label: '自定义文本', enabled: true },
    { key: 'size', label: '文件大小', enabled: false },
    { key: 'device', label: '拍摄设备', enabled: false },
    { key: 'takenAt', label: '拍摄时间', enabled: false },
    { key: 'focal', label: '拍摄焦距', enabled: false }
  ])

  const customText = ref('ProjectX')
  const separator = ref('_')
  const durationFormat = ref<DurationFormat>('hms')
  const leadingZeros = ref(2)

  const videoRows = ref<VideoRow[]>([])
  const imageRows = ref<ImageRow[]>([])

  const visibleVideoColumns = reactive({
    duration: true,
    resolution: true,
    bitrate: true,
    size: true,
    preview: false
  })

  const visibleImageColumns = reactive({
    resolution: true,
    device: false,
    takenAt: false,
    focalLength: false,
    size: true,
    preview: false
  })

  const showPreview = ref(false)
  const liveImports = ref<LiveImportItem[]>([])
  const lastImportKind = ref<ImportKind>('file')
  const lastRenameBatch = ref<UndoRenameItem[][]>([])
  let unlistenProgress: (() => void) | null = null
  const videoSort = ref<{ prop: string | null; order: 'ascending' | 'descending' | null }>({ prop: null, order: null })
  const imageSort = ref<{ prop: string | null; order: 'ascending' | 'descending' | null }>({ prop: null, order: null })

  // 初始化时检测 MediaInfo 状态
  onMounted(async () => {
    if (!hasTauriRuntime()) return

    try {
      mediaInfoStatus.value = await getMediaInfoStatus()
    } catch (e) {
      console.error('Failed to get MediaInfo status:', e)
    }

    if (!unlistenProgress) {
      unlistenProgress = await listen('progress-update', (event) => {
        const payload = event.payload as any
        if (!payload || payload.stage !== 'media_import') return
        const total = Math.max(1, Number(payload.total || 1))
        const current = Math.min(total, Number(payload.current || 0))
        importProgress.active = true
        importProgress.totalItems = total
        importProgress.totalBatches = Math.max(1, Math.ceil(total / batchSize.value))
        importProgress.percent = Math.max(0, Math.min(100, Math.floor((current / total) * 100)))
        importProgress.batch = Math.max(1, Math.min(importProgress.totalBatches, Math.ceil(current / batchSize.value) || 1))
      })
    }
  })

  onBeforeUnmount(() => {
    unlistenProgress?.()
    unlistenProgress = null
  })

  const formatPreviewName = (row: VideoRow | ImageRow, index: number) => {
    const parts: string[] = []
    const seq = String(index + 1).padStart(leadingZeros.value, '0')
    const fields = row.mediaType === 'video' ? renameFieldsVideo : renameFieldsImage
    const ext = row.name.includes('.') ? row.name.slice(row.name.lastIndexOf('.') + 1) : ''
    fields.forEach((field) => {
      if (!field.enabled) return
      switch (field.key) {
        case 'seq':
          parts.push(seq)
          break
        case 'custom':
          if (customText.value.trim()) parts.push(customText.value.trim())
          break
        case 'duration':
          if (row.mediaType === 'video') parts.push(formatDuration((row as VideoRow).durationSec, durationFormat.value))
          break
        case 'resolution':
          parts.push(`${row.width ?? '-'}x${row.height ?? '-'}`)
          break
        case 'bitrate':
          if (row.mediaType === 'video') {
            const val = (row as VideoRow).bitrateMbps
            parts.push(val ? `${val.toFixed(1)}Mbps` : '0Mbps')
          }
          break
        case 'device':
          if (row.mediaType === 'image') parts.push((row as ImageRow).device || '-')
          break
        case 'takenAt':
          if (row.mediaType === 'image') parts.push((row as ImageRow).takenAt || '-')
          break
        case 'focal':
          if (row.mediaType === 'image') parts.push((row as ImageRow).focalLength || '-')
          break
        case 'size':
          parts.push(formatBytes(row.size))
          break
        case 'filename':
          parts.push(row.name)
          break
        default:
          break
      }
    })
    const base = parts.join(separator.value || '_').trim() || `unnamed_${seq}`
    if (!ext) return base
    const normalized = base.toLowerCase()
    if (normalized.endsWith(`.${ext.toLowerCase()}`)) return base
    return `${base}.${ext}`
  }

  const compareByOrder = (a: number, b: number, order: 'ascending' | 'descending' | null) => {
    if (!order) return 0
    return order === 'ascending' ? a - b : b - a
  }

  const sortVideoRows = (rows: VideoRow[]) => {
    const { prop, order } = videoSort.value
    if (!prop || !order) return [...rows]
    return [...rows].sort((a, b) => {
      if (prop === 'durationSec') return compareByOrder(a.durationSec || 0, b.durationSec || 0, order)
      if (prop === 'bitrateMbps') return compareByOrder(a.bitrateMbps || 0, b.bitrateMbps || 0, order)
      if (prop === 'size') return compareByOrder(a.size || 0, b.size || 0, order)
      if (prop === 'resolution') return compareByOrder((a.width || 0) * (a.height || 0), (b.width || 0) * (b.height || 0), order)
      return 0
    })
  }

  const sortImageRows = (rows: ImageRow[]) => {
    const { prop, order } = imageSort.value
    if (!prop || !order) return [...rows]
    return [...rows].sort((a, b) => {
      if (prop === 'size') return compareByOrder(a.size || 0, b.size || 0, order)
      if (prop === 'resolution') return compareByOrder((a.width || 0) * (a.height || 0), (b.width || 0) * (b.height || 0), order)
      return 0
    })
  }

  const handleTableSortChange = (
    kind: MediaKind,
    payload: { prop: string | null; order: 'ascending' | 'descending' | null }
  ) => {
    if (kind === 'video') {
      videoSort.value = payload
    } else {
      imageSort.value = payload
    }
  }

  const tableVideos = computed(() =>
    sortVideoRows(videoRows.value).map((row, index) => ({
      ...row,
      previewName: formatPreviewName(row, index),
      order: index + 1
    }))
  )

  const tableImages = computed(() =>
    sortImageRows(imageRows.value).map((row, index) => ({
      ...row,
      previewName: formatPreviewName(row, index),
      order: index + 1
    }))
  )

  const activeRenameRows = computed<RenameRow[]>(() =>
    fileTypeTab.value === 'video' ? tableVideos.value : tableImages.value
  )

  const canUndoRename = computed(() => lastRenameBatch.value.length > 0)
  const undoStackDepth = computed(() => lastRenameBatch.value.length)

  const basicStats = computed(() => {
    const successVideos = videoRows.value.filter((item) => item.status === 'success')
    const successImages = imageRows.value.filter((item) => item.status === 'success')
    const totalVideos = successVideos.length
    const totalImages = successImages.length
    const totalDuration = successVideos.reduce((sum, item) => sum + (item.durationSec || 0), 0)
    const totalSize = [...successVideos, ...successImages].reduce((sum, item) => sum + item.size, 0)
    const avgDuration = totalVideos ? totalDuration / totalVideos : 0
    return {
      totalVideos,
      totalImages,
      totalDuration: formatDuration(totalDuration, durationFormat.value),
      avgDuration: formatDuration(avgDuration, durationFormat.value),
      totalSize: formatBytes(totalSize)
    }
  })

  const advancedStats = computed(() => {
    if (fileTypeTab.value === 'video') {
      const vids = videoRows.value.filter((item) => item.status === 'success')
      if (!vids.length) {
        return { longest: '-', shortest: '-', maxBitrate: '-', minBitrate: '-', avgBitrate: '-', avgDuration: '-' }
      }
      const longest = vids.reduce((a, b) => ((b.durationSec || 0) > (a.durationSec || 0) ? b : a))
      const shortest = vids.reduce((a, b) => ((b.durationSec || 0) < (a.durationSec || 0) ? b : a))
      const maxBitrate = vids.reduce((a, b) => ((b.bitrateMbps || 0) > (a.bitrateMbps || 0) ? b : a))
      const minBitrate = vids.reduce((a, b) => {
        if ((a.bitrateMbps || 0) === 0) return b
        if ((b.bitrateMbps || 0) === 0) return a
        return (b.bitrateMbps || 0) < (a.bitrateMbps || 0) ? b : a
      })
      const avgBitrateVal =
        vids.reduce((sum, v) => sum + (v.bitrateMbps || 0), 0) / (vids.filter((v) => v.bitrateMbps).length || vids.length)
      const avgDurationVal = vids.reduce((sum, v) => sum + (v.durationSec || 0), 0) / vids.length
      return {
        longest: `${longest.name} (${formatDuration(longest.durationSec, durationFormat.value)})`,
        shortest: `${shortest.name} (${formatDuration(shortest.durationSec, durationFormat.value)})`,
        maxBitrate: `${maxBitrate.name} (${(maxBitrate.bitrateMbps || 0).toFixed(2)} Mbps)`,
        minBitrate: `${minBitrate.name} (${(minBitrate.bitrateMbps || 0).toFixed(2)} Mbps)`,
        avgBitrate: `${avgBitrateVal.toFixed(2)} Mbps`,
        avgDuration: formatDuration(avgDurationVal, durationFormat.value)
      }
    }

    const imgs = imageRows.value.filter((item) => item.status === 'success')
    if (!imgs.length) {
      return { maxResolution: '-', minResolution: '-', totalImages: 0, formatCounts: [] as { ext: string; count: number }[] }
    }
    const maxRes = imgs.reduce((a, b) => ((b.width || 0) * (b.height || 0) > (a.width || 0) * (a.height || 0) ? b : a))
    const minRes = imgs.reduce((a, b) => ((b.width || 0) * (b.height || 0) < (a.width || 0) * (a.height || 0) ? b : a))
    const counts: Record<string, number> = {}
    imgs.forEach((img) => {
      const ext = img.name.split('.').pop()?.toLowerCase() || '未知'
      counts[ext] = (counts[ext] || 0) + 1
    })
    const formatCounts = Object.entries(counts).map(([ext, count]) => ({ ext, count }))
    return {
      maxResolution: `${maxRes.name} (${maxRes.width || '-'}×${maxRes.height || '-'})`,
      minResolution: `${minRes.name} (${minRes.width || '-'}×${minRes.height || '-'})`,
      totalImages: imgs.length,
      formatCounts
    }
  })

  const durationBuckets = computed(() => {
    const STEP_SECONDS = 5 * 60
    const successVideos = videoRows.value.filter((item) => item.status === 'success')
    const maxDuration = successVideos.reduce((acc, item) => Math.max(acc, item.durationSec || 0), 0)
    const steps = Math.max(1, Math.ceil(maxDuration / STEP_SECONDS))

    const ranges = Array.from({ length: steps }, (_, idx) => {
      const min = idx * STEP_SECONDS
      const max = min + STEP_SECONDS
      return {
        label: `${idx * 5}-${(idx + 1) * 5}min`,
        min,
        max
      }
    })

    const buckets = ranges
      .map((range) => ({
        ...range,
        count: successVideos.filter((item) => {
          const duration = item.durationSec || 0
          return duration >= range.min && duration < range.max
        }).length
      }))
      .filter((range) => range.count > 0)

    const longCount = successVideos.filter((item) => (item.durationSec || 0) >= 3600).length
    if (longCount > 0) {
      buckets.push({ label: '60min+', min: 3600, max: Infinity, count: longCount })
    }

    return buckets
  })

  const moveField = (list: RenameField[], index: number, direction: 'up' | 'down') => {
    const target = index + (direction === 'up' ? -1 : 1)
    if (target < 0 || target >= list.length) return
    const current = list[index]
    const next = list[target]
    if (!current || !next) return
    list[index] = next
    list[target] = current
  }

  const startProgress = (pathsCount: number) => {
    importProgress.active = true
    importProgress.batch = 1
    importProgress.totalItems = pathsCount
    importProgress.totalBatches = Math.max(1, Math.ceil(pathsCount / batchSize.value))
    importProgress.percent = 0
  }

  const finishProgress = () => {
    importProgress.percent = 100
    importProgress.batch = importProgress.totalBatches
    importProgress.active = false
  }

  const detectMediaKind = (path: string): MediaKind | null => {
    const ext = path.split('.').pop()?.toLowerCase() || ''
    if (VIDEO_EXTS.includes(ext)) return 'video'
    if (IMAGE_EXTS.includes(ext)) return 'image'
    return null
  }

  const buildPendingRows = (paths: string[], source: ImportKind) => {
    const pendingVideos: VideoRow[] = []
    const pendingImages: ImageRow[] = []
    if (source === 'folder') {
      return { pendingVideos, pendingImages }
    }
    paths.forEach((path, idx) => {
      const name = path.split(/[\\/]/).pop() || path
      const mediaType = detectMediaKind(path)
      if (!mediaType) return
      const base = {
        id: idx + 1,
        name,
        path,
        size: 0,
        mediaType,
        status: 'pending' as const,
        reason: '正在提取中...'
      }
      if (mediaType === 'video') {
        pendingVideos.push({
          ...base,
          width: 0,
          height: 0,
          durationSec: 0,
          bitrateMbps: 0,
        })
      } else {
        pendingImages.push({
          ...base,
          width: 0,
          height: 0,
          device: '',
          takenAt: '',
          focalLength: ''
        })
      }
    })
    return { pendingVideos, pendingImages }
  }

  const clearPendingPlaceholders = () => {
    videoRows.value = videoRows.value.filter((row) => row.status !== 'pending')
    imageRows.value = imageRows.value.filter((row) => row.status !== 'pending')
  }

  const setLiveImportSnapshot = (paths: string[], source: ImportKind) => {
    if (source === 'folder') {
      liveImports.value = []
      lastImportKind.value = source
      return
    }
    const stamp = Date.now()
    lastImportKind.value = source
    liveImports.value = paths.slice(0, LIVE_IMPORT_SNAPSHOT_LIMIT).map((path, idx) => ({
      id: `${stamp}-${idx}`,
      name: path.split(/[\\/]/).pop() || path,
      path,
      source,
      status: 'pending'
    }))
  }

  const syncLiveImportStatus = (successPaths: Set<string>, failedPaths: Set<string>) => {
    if (!liveImports.value.length) return
    liveImports.value = liveImports.value.map((item) => {
      if (failedPaths.has(item.path)) return { ...item, status: 'failed' }
      if (successPaths.has(item.path)) return { ...item, status: 'success' }
      return item
    })
  }

  const setLiveImportsFromItems = (items: (VideoRow | ImageRow)[]) => {
    const stamp = Date.now()
    liveImports.value = items.slice(0, LIVE_IMPORT_DISPLAY_LIMIT).map((item, idx) => ({
      id: `${stamp}-${idx}`,
      name: item.name,
      path: item.path,
      source: lastImportKind.value,
      status: item.status === 'success' ? 'success' : item.status === 'error' ? 'failed' : 'pending'
    }))
  }

  const mergeByPath = <T extends VideoRow | ImageRow>(prev: T[], next: T[]) => {
    const map = new Map<string, T>()
    prev.forEach((item) => map.set(item.path, item))
    next.forEach((item) => map.set(item.path, item))
    return Array.from(map.values()).map((item, idx) => ({ ...item, id: idx + 1 } as T))
  }

  // 导入完成后的收尾定时器句柄；新一轮导入前清除，避免上一轮残留回调
  // 提前隐藏进度条或清空实时清单。
  const importFinishTimers: number[] = []
  const clearImportTimers = () => {
    importFinishTimers.forEach((t) => window.clearTimeout(t))
    importFinishTimers.length = 0
  }
  const scheduleImportFinish = (fn: () => void, delay: number) => {
    importFinishTimers.push(window.setTimeout(fn, delay))
  }

  // 核心导入逻辑：接收已确定的路径列表，供 handleImport 与跨视图联动复用
  const importPaths = async (paths: string[], kind: ImportKind = 'file') => {
    if (importing.value) return
    clearImportTimers()
    if (!paths || paths.length === 0) {
      liveImports.value = []
      finishProgress()
      importing.value = false
      return
    }
    setLiveImportSnapshot(paths, kind)
    importing.value = true
    lastRenameBatch.value = []
    failedItems.value = []
    importSummary.value = ''
    const pending = buildPendingRows(paths, kind)
    videoRows.value = mergeByPath(videoRows.value, pending.pendingVideos)
    imageRows.value = mergeByPath(imageRows.value, pending.pendingImages)
    startProgress(paths.length)
    try {
      const resp = await importMedia(paths, recursive.value)
      const videos = resp.items.filter((item) => item.mediaType === 'video') as VideoRow[]
      const images = resp.items.filter((item) => item.mediaType === 'image') as ImageRow[]

      clearPendingPlaceholders()
      videoRows.value = mergeByPath(videoRows.value, videos)
      imageRows.value = mergeByPath(imageRows.value, images)
      setLiveImportsFromItems(resp.items)
      failedItems.value = resp.items.filter((item) => item.status !== 'success') as (VideoRow | ImageRow)[]
      importProgress.totalItems = resp.stats.total
      importProgress.totalBatches = Math.max(1, Math.ceil((resp.stats.total as number) / batchSize.value))
      const makeGroupedFormat = () => {
        const sorted = [...(resp.stats.formatCounts || [])].sort((a, b) => {
          if (b.count !== a.count) return b.count - a.count
          return a.ext.localeCompare(b.ext)
        })
        const videosGrouped = sorted
          .filter((f) => VIDEO_EXTS.includes(f.ext))
          .map((f) => `${f.ext}(${f.count})`)
          .join(', ')
        const imagesGrouped = sorted
          .filter((f) => IMAGE_EXTS.includes(f.ext))
          .map((f) => `${f.ext}(${f.count})`)
          .join(', ')
        const vStr = videosGrouped || t('无')
        const iStr = imagesGrouped || t('无')
        return `视频{${vStr}} 图片{${iStr}}`
      }
      importSummary.value = t('成功 {success} 个，失败 {failed} 个；格式统计：{formats}', {
        success: resp.stats.success,
        failed: resp.stats.failed,
        formats: makeGroupedFormat()
      })
      const successPaths = new Set(resp.items.filter((item) => item.status === 'success').map((item) => item.path))
      const failedPaths = new Set(resp.items.filter((item) => item.status !== 'success').map((item) => item.path))
      syncLiveImportStatus(successPaths, failedPaths)
      scheduleImportFinish(() => finishProgress(), 1200)
      if (resp.stats.failed > 0) {
        ElMessage.warning(`${t('导入完成')}：${importSummary.value}`)
      } else {
        ElMessage.success(`${t('导入完成')}：${importSummary.value}`)
      }
    } catch (error: any) {
      syncLiveImportStatus(new Set(), new Set(paths))
      clearPendingPlaceholders()
      scheduleImportFinish(() => finishProgress(), 1600)
      ElMessage.error(error?.toString() || t('导入失败'))
    } finally {
      importing.value = false
      scheduleImportFinish(() => { liveImports.value = [] }, 1300)
    }
  }

  const handleImport = async (kind: ImportKind) => {
    if (importing.value) return
    clearImportTimers()
    // 先拿到路径，再开启 loading，避免选择/剪贴板为空导致一直转圈
    const picked = await pick(kind)
    const paths = (picked || []).filter((p: string) => !/[\*\?\[\]]/.test(p))
    if (picked && paths.length !== picked.length) {
      ElMessage.warning(t('已忽略包含通配符的路径'))
    }
    return importPaths(paths, kind)
  }

  const handleRemove = async (kind: MediaKind, id: number) => {
    if (kind === 'video') {
      videoRows.value = videoRows.value.filter((row) => row.id !== id)
    } else {
      imageRows.value = imageRows.value.filter((row) => row.id !== id)
    }
    ElMessage.success(t('已从列表移除'))
  }

  const handleClear = async (kind: MediaKind) => {
    const confirmed = await ElMessageBox.confirm(
      `${t('清空确认')} - ${kind === 'video' ? t('视频') : t('图片')}`,
      t('清空确认'),
      {
        confirmButtonText: t('清空'),
        cancelButtonText: t('取消'),
        type: 'warning'
      }
    ).catch(() => false)
    if (!confirmed) return
    if (kind === 'video') {
      videoRows.value = []
    } else {
      imageRows.value = []
    }
    lastRenameBatch.value = []
    ElMessage.success(t('列表已清空'))
  }

  const refreshStats = () => {
    if (allowAutoRefresh.value) {
      ElMessage.success(t('已刷新统计数据'))
    }
  }

  const copyName = async (name: string) => {
    try {
      await writeText(name)
      ElMessage.success(t('已复制名称'))
    } catch (e: any) {
      ElMessage.error(e?.toString() || t('复制失败'))
    }
  }

  const openFolder = async (path: string) => {
    try {
      await openParentDir(path)
    } catch (e: any) {
      ElMessage.error(e?.toString() || t('打开失败'))
    }
  }

  const previewRename = () => {
    showPreview.value = true
    if (fileTypeTab.value === 'video') {
      visibleVideoColumns.preview = true
    } else {
      visibleImageColumns.preview = true
    }
  }

  const sanitizeName = (name: string) => {
    const invalid = /[<>:"/\\|?*\r\n]/g
    const cleaned = name.replace(invalid, '_').trim()
    return cleaned || 'unnamed'
  }

  const splitPath = (path: string) => {
    const idx = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
    return {
      dir: idx >= 0 ? path.slice(0, idx + 1) : '',
      name: idx >= 0 ? path.slice(idx + 1) : path
    }
  }

  const appendSuffix = (fileName: string, counter: number) => {
    const dot = fileName.lastIndexOf('.')
    const stem = dot > 0 ? fileName.slice(0, dot) : fileName
    const ext = dot > 0 ? fileName.slice(dot) : ''
    return `${stem}(${counter})${ext}`
  }

  const makeTargetKey = (path: string) => path.toLowerCase()

  const renameSafetySummary = computed<RenameSafetySummary>(() => {
    if (!showPreview.value) {
      return { total: 0, readyCount: 0, unchangedCount: 0, illegalNameCount: 0, duplicateTargetCount: 0 }
    }

    const rows = activeRenameRows.value
    const targetCounts = new Map<string, number>()
    rows.forEach((row) => {
      const { dir } = splitPath(row.path)
      const safeName = sanitizeName(row.previewName)
      const targetKey = makeTargetKey(`${dir}${safeName}`)
      targetCounts.set(targetKey, (targetCounts.get(targetKey) || 0) + 1)
    })

    let unchangedCount = 0
    let illegalNameCount = 0
    let duplicateTargetCount = 0
    rows.forEach((row) => {
      const { dir } = splitPath(row.path)
      const safeName = sanitizeName(row.previewName)
      const targetPath = `${dir}${safeName}`
      if (safeName !== row.previewName) illegalNameCount++
      if (targetPath === row.path) unchangedCount++
      if ((targetCounts.get(makeTargetKey(targetPath)) || 0) > 1) duplicateTargetCount++
    })

    return {
      total: rows.length,
      readyCount: Math.max(0, rows.length - unchangedCount),
      unchangedCount,
      illegalNameCount,
      duplicateTargetCount
    }
  })

  const buildRenamePlan = async (rows: RenameRow[], checkFileSystem: boolean): Promise<RenamePlanItem[]> => {
    const plannedTargets = new Set<string>()
    const baseTargetCounts = new Map<string, number>()
    rows.forEach((row) => {
      const { dir } = splitPath(row.path)
      const safeName = sanitizeName(row.previewName)
      const baseTarget = `${dir}${safeName}`
      const key = makeTargetKey(baseTarget)
      baseTargetCounts.set(key, (baseTargetCounts.get(key) || 0) + 1)
    })

    const plan: RenamePlanItem[] = []
    for (const row of rows) {
      const { dir, name: sourceName } = splitPath(row.path)
      const safeName = sanitizeName(row.previewName)
      const warnings: string[] = []
      if (safeName !== row.previewName) {
        warnings.push(t('文件名包含非法字符，已自动替换'))
      }

      const baseTargetPath = `${dir}${safeName}`
      if ((baseTargetCounts.get(makeTargetKey(baseTargetPath)) || 0) > 1) {
        warnings.push(t('同批目标名称重复，将自动追加序号'))
      }

      let targetName = safeName
      let targetPath = baseTargetPath
      let counter = 1
      while (
        targetPath !== row.path &&
        (plannedTargets.has(makeTargetKey(targetPath)) || (checkFileSystem && await pathExists(targetPath)))
      ) {
        if (counter === 1) warnings.push(t('目标文件已存在，将自动追加序号'))
        targetName = appendSuffix(safeName, counter)
        targetPath = `${dir}${targetName}`
        counter++
        if (counter > 9999) {
          targetName = appendSuffix(safeName, Date.now())
          targetPath = `${dir}${targetName}`
          break
        }
      }

      plannedTargets.add(makeTargetKey(targetPath))
      plan.push({
        row,
        sourcePath: row.path,
        sourceName,
        targetPath,
        targetName,
        unchanged: targetPath === row.path,
        warnings
      })
    }
    return plan
  }

  const updateRowAfterRename = (kind: MediaKind, oldPath: string, newPath: string, newName: string) => {
    const target = kind === 'video' ? videoRows : imageRows
    const idx = target.value.findIndex((row) => row.path === oldPath)
    if (idx < 0) return
    target.value[idx] = {
      ...target.value[idx]!,
      path: newPath,
      name: newName
    } as any
  }

  const applyRename = async () => {
    if (!showPreview.value) {
      ElMessage.info(t('请先点击“预览重命名”'))
      return
    }
    const kind = fileTypeTab.value
    const list = activeRenameRows.value
    if (!list.length) {
      ElMessage.info(t('没有可重命名的文件'))
      return
    }

    let success = 0
    const failed: { name: string; reason: string }[] = []
    const plan = await buildRenamePlan(list, true)
    const warnings = plan.flatMap((item) => item.warnings)

    if (warnings.length) {
      const confirmed = await ElMessageBox.confirm(
        t('检测到 {count} 个命名风险，已生成自动避让方案。是否继续应用？', { count: warnings.length }),
        t('重命名预检'),
        {
          confirmButtonText: t('继续应用'),
          cancelButtonText: t('取消'),
          type: 'warning'
        }
      ).catch(() => false)
      if (!confirmed) return
    }

    try {
      const undoItems: UndoRenameItem[] = []
      const BATCH_SIZE = 10
      for (let i = 0; i < plan.length; i += BATCH_SIZE) {
        const batch = plan.slice(i, i + BATCH_SIZE)
        const results = await Promise.allSettled(
          batch.map(async (item) => {
            if (item.unchanged) return { type: 'unchanged' as const, item }
            await renamePath(item.sourcePath, item.targetPath)
            return { type: 'renamed' as const, item }
          })
        )
        for (let j = 0; j < results.length; j++) {
          const result = results[j]!
          const item = batch[j]!
          if (result.status === 'fulfilled') {
            if (result.value.type === 'renamed') {
              updateRowAfterRename(kind, item.sourcePath, item.targetPath, item.targetName)
              undoItems.push({
                originalPath: item.sourcePath,
                originalName: item.sourceName,
                currentPath: item.targetPath,
                currentName: item.targetName,
                kind
              })
            }
            success++
          } else {
            failed.push({ name: item.sourceName, reason: result.reason?.toString() || t('重命名失败') })
          }
        }
      }
      lastRenameBatch.value.push(undoItems)

      if (failed.length) {
        ElMessage.warning(
          t('重命名完成：成功 {success} 个，失败 {failed} 个', { success, failed: failed.length })
        )
      } else {
        ElMessage.success(t('重命名完成：成功 {success} 个', { success }))
      }
    } catch (e: any) {
      ElMessage.error(e?.toString() || t('重命名异常'))
    }
  }

  const undoLastRename = async () => {
    if (!lastRenameBatch.value.length) {
      ElMessage.info(t('没有可撤销的重命名记录'))
      return
    }
    const batch = lastRenameBatch.value[lastRenameBatch.value.length - 1]!
    const confirmed = await ElMessageBox.confirm(
      t('将撤销上一次重命名，共 {count} 个文件。是否继续？', { count: batch.length }),
      t('撤销重命名'),
      {
        confirmButtonText: t('撤销'),
        cancelButtonText: t('取消'),
        type: 'warning'
      }
    ).catch(() => false)
    if (!confirmed) return

    let success = 0
    const failed: { name: string; reason: string }[] = []
    const failedIndices = new Set<number>()
    for (let i = batch.length - 1; i >= 0; i--) {
      const item = batch[i]
      if (!item) continue
      try {
        if (!await pathExists(item.currentPath)) {
          failed.push({ name: item.currentName, reason: t('当前文件不存在') })
          failedIndices.add(i)
          continue
        }
        if (await pathExists(item.originalPath)) {
          failed.push({ name: item.currentName, reason: t('原路径已有文件，已跳过') })
          failedIndices.add(i)
          continue
        }
        await renamePath(item.currentPath, item.originalPath)
        updateRowAfterRename(item.kind, item.currentPath, item.originalPath, item.originalName)
        success++
      } catch (e: any) {
        failed.push({ name: item.currentName, reason: e?.toString() || t('撤销失败') })
        failedIndices.add(i)
      }
    }

    // pop the current layer from stack
    lastRenameBatch.value.pop()

    if (failed.length) {
      // push back only the failed items as a new layer for potential retry
      const remaining = batch.filter((_, i) => failedIndices.has(i))
      if (remaining.length) lastRenameBatch.value.push(remaining)
      ElMessage.warning(t('撤销完成：成功 {success} 个，失败 {failed} 个', { success, failed: failed.length }))
    } else {
      ElMessage.success(t('撤销完成：成功 {success} 个', { success }))
    }
  }

  const toggleAllColumns = (kind: MediaKind, field?: string, value?: boolean) => {
    const cols = kind === 'video' ? visibleVideoColumns : visibleImageColumns
    if (field && value !== undefined) {
      ;(cols as any)[field] = value
    } else {
      const allOn = Object.values(cols).every(Boolean)
      Object.keys(cols).forEach((key) => {
        ;(cols as any)[key] = !allOn
      })
    }
  }

  const toggleRenameFields = (kind: MediaKind) => {
    const list = kind === 'video' ? renameFieldsVideo : renameFieldsImage
    const allOn = list.every((field) => field.enabled)
    list.forEach((field) => {
      field.enabled = !allOn
    })
  }

  const toggleRenameField = (kind: MediaKind, index: number, enabled: boolean) => {
    const list = kind === 'video' ? renameFieldsVideo : renameFieldsImage
    if (list[index]) {
      list[index].enabled = enabled
    }
  }

  const setRenameEnabled = (list: RenameField[], keys: string[]) => {
    const enabled = new Set(keys)
    list.forEach((field) => {
      field.enabled = enabled.has(field.key)
    })
  }

  const applyOrganizePreset = (preset: OrganizePresetKey) => {
    // 先切换 tab（会触发 watch(fileTypeTab) 重置列与 showPreview）。
    // 列与 showPreview 的赋值推迟到 nextTick，确保在 watch 回调之后执行，
    // 避免预设设置被 watch 的默认值覆盖；若 tab 未变化则 watch 不触发，直接应用。
    const applyColumns = () => {
      if (preset === 'short-video') {
        setRenameEnabled(renameFieldsVideo, ['seq', 'custom', 'duration', 'resolution'])
        Object.assign(visibleVideoColumns, {
          duration: true,
          resolution: true,
          bitrate: true,
          size: true,
          preview: true
        })
      } else if (preset === 'archive-video') {
        setRenameEnabled(renameFieldsVideo, ['seq', 'custom', 'filename', 'duration', 'size'])
        Object.assign(visibleVideoColumns, {
          duration: true,
          resolution: true,
          bitrate: true,
          size: true,
          preview: true
        })
      } else {
        setRenameEnabled(renameFieldsImage, ['seq', 'custom', 'takenAt', 'device', 'resolution'])
        Object.assign(visibleImageColumns, {
          resolution: true,
          device: true,
          takenAt: true,
          focalLength: true,
          size: true,
          preview: true
        })
      }
      showPreview.value = true
    }

    const targetTab: 'video' | 'image' = preset === 'photo-exif' ? 'image' : 'video'
    const tabChanged = fileTypeTab.value !== targetTab

    if (preset === 'short-video') {
      fileTypeTab.value = 'video'
      customText.value = 'clip'
      separator.value = '_'
      leadingZeros.value = 3
      durationFormat.value = 'clock'
    } else if (preset === 'archive-video') {
      fileTypeTab.value = 'video'
      customText.value = 'archive'
      separator.value = '_'
      leadingZeros.value = 4
      durationFormat.value = 'hms'
    } else {
      fileTypeTab.value = 'image'
      customText.value = 'photo'
      separator.value = '_'
      leadingZeros.value = 3
    }
    if (tabChanged) {
      // tab 变化时 watch 会重置，需在 nextTick 之后重新应用列设置。
      nextTick(applyColumns)
    } else {
      applyColumns()
    }
    ElMessage.success(t('已应用整理模板'))
  }

  watch(fileTypeTab, () => {
    showPreview.value = false
    if (fileTypeTab.value === 'video') {
      visibleVideoColumns.preview = false
      visibleVideoColumns.duration = true
      visibleVideoColumns.resolution = true
      visibleVideoColumns.bitrate = true
      visibleVideoColumns.size = true
    } else {
      visibleImageColumns.preview = false
      visibleImageColumns.resolution = true
      visibleImageColumns.device = false
      visibleImageColumns.takenAt = false
      visibleImageColumns.focalLength = false
      visibleImageColumns.size = true
    }
  })

  const retryFailedImports = async () => {
    if (importing.value) return
    const retryPaths = failedItems.value.map((item) => item.path).filter(Boolean)
    if (!retryPaths.length) {
      ElMessage.info(t('没有可重试的失败项'))
      return
    }
    clearImportTimers()
    importing.value = true
    startProgress(retryPaths.length)
    try {
      const resp = await importMedia(retryPaths, recursive.value)
      const videos = resp.items.filter((item) => item.mediaType === 'video') as VideoRow[]
      const images = resp.items.filter((item) => item.mediaType === 'image') as ImageRow[]
      videoRows.value = mergeByPath(videoRows.value, videos)
      imageRows.value = mergeByPath(imageRows.value, images)
      failedItems.value = resp.items.filter((item) => item.status !== 'success') as (VideoRow | ImageRow)[]
      finishProgress()
      ElMessage.success(t('失败项重试完成'))
    } catch (error: any) {
      finishProgress()
      ElMessage.error(error?.toString() || t('重试失败'))
    } finally {
      importing.value = false
    }
  }

  return {
    mediaInfoStatus,
    fileTypeTab,
    batchSize,
    allowAutoRefresh,
    recursive,
    importing,
    importProgress,
    renameFieldsVideo,
    renameFieldsImage,
    customText,
    separator,
    durationFormat,
    leadingZeros,
    videoRows,
    imageRows,
    visibleVideoColumns,
    visibleImageColumns,
    tableVideos,
    tableImages,
    basicStats,
    advancedStats,
    durationBuckets,
    showPreview,
    renameSafetySummary,
    canUndoRename,
    undoStackDepth,
    formatBytes,
    formatDuration,
    handleImport,
    importPaths,
    handleRemove,
    handleClear,
    moveField,
    refreshStats,
    copyName,
    openFolder,
    previewRename,
    applyRename,
    undoLastRename,
    handleTableSortChange,
    toggleAllColumns,
    toggleRenameFields,
    toggleRenameField,
    applyOrganizePreset,
    failedItems,
    failedReasonStats,
    showFailedDialog,
    retryFailedImports,
    importSummary,
    liveImports,
    mergeByPath
  }
}
