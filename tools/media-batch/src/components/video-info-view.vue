<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, shallowRef } from 'vue'
import { ElMessage } from 'element-plus'
import { FolderAdd, Upload, Grid, List, Filter, Download, Coin } from '@element-plus/icons-vue'
import { save, open as openDialog } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { useFileSelect } from '@core/hooks/useFileSelect'
import { useSettings } from '@core/hooks/useSettings'
import { formatBytes } from '@core/utils/format'
import { hasTauriRuntime } from '@core/utils/tauri'
import { writeBinaryExportFile, writeTextExportFile } from '@core/api/common'
import { importDetailedVideoInfo, getVideoRawXml, getVideoCompleteInfo, getVideoXmlJson, getVideoXmlMarkdown } from '../api/media-batch'
import { saveVideoRecords, loadVideoRecords, deleteVideoRecords, getRecordCount, rowToVideoInfoItem, type VideoRecordRow } from '../api/video-db'
import VideoInfoDetailCard from './video-info-detail-card.vue'
import type { DisplayLevel, VideoInfoImportResponse, VideoInfoItem } from '../types/media'

type ExportFormat = 'xlsx' | 'csv' | 'markdown' | 'html' | 'json' | 'txt' | 'xml2json' | 'xml2md'
type HealthSeverity = 'danger' | 'warning' | 'info'

// MediaInfo 语言名称 -> 中文映射
const langMap: Record<string, string> = {
  'chinese': '中文',
  'chinese (simplified)': '简体中文',
  'chinese (traditional)': '繁体中文',
  'english': '英文',
  'japanese': '日语',
  'korean': '韩语',
  'french': '法语',
  'german': '德语',
  'spanish': '西班牙语',
  'russian': '俄语',
  'arabic': '阿拉伯语',
  'portuguese': '葡萄牙语',
  'italian': '意大利语',
  'thai': '泰语',
  'vietnamese': '越南语',
  'indonesian': '印尼语',
  'malay': '马来语',
  'hindi': '印地语',
  'dutch': '荷兰语',
  'polish': '波兰语',
  'turkish': '土耳其语',
  'swedish': '瑞典语',
  'norwegian': '挪威语',
  'danish': '丹麦语',
  'finnish': '芬兰语',
  'greek': '希腊语',
  'hebrew': '希伯来语',
  'czech': '捷克语',
  'romanian': '罗马尼亚语',
  'hungarian': '匈牙利语',
  'ukrainian': '乌克兰语',
}

const translateLang = (lang: string): string => {
  if (!lang) return lang
  return langMap[lang.toLowerCase()] || lang
}
type HealthIssue = {
  severity: HealthSeverity
  label: string
  detail: string
}

const { pick } = useFileSelect()
const { t } = useSettings()
const videoInfoCache = new Map<string, VideoInfoImportResponse>()

const level = ref<DisplayLevel>('beginner')
const recursive = ref(false)
const importing = ref(false)
const items = shallowRef<VideoInfoItem[]>([])
const expandedIds = ref<Set<number>>(new Set())
const importSummary = ref('')

// 批量导出输出目录模式
type OutputMode = 'same' | 'folder'
const outputModeDialogVisible = ref(false)
const pendingExportFormat = ref<ExportFormat | null>(null)
const outputMode = ref<OutputMode>('same')
const customOutputDir = ref('')

// 数据库相关状态
const saving = ref(false)
const saveProgress = reactive({ total: 0, saved: 0, phase: 'xml' as 'xml' | 'db' | 'done' })
const historyVisible = ref(false)
const historyRecords = shallowRef<VideoRecordRow[]>([])
const historyLoading = ref(false)
const historySelection = ref<string[]>([])
const historyCount = ref(0)
const historyPageSize = 50
const historyHasMore = ref(true)

// 历史记录列选择
type HistoryColDef = { key: string; label: string; width?: number; group: 'basic' | 'video' | 'audio' | 'extra' }
const HISTORY_ALL_COLUMNS: HistoryColDef[] = [
  { key: 'name', label: '文件名', width: 200, group: 'basic' },
  { key: 'format', label: '格式', width: 80, group: 'basic' },
  { key: 'resolution', label: '分辨率', width: 120, group: 'basic' },
  { key: 'codec', label: '编码', width: 100, group: 'basic' },
  { key: 'frame_rate', label: '帧率', width: 80, group: 'basic' },
  { key: 'duration', label: '时长', width: 90, group: 'basic' },
  { key: 'overall_bit_rate', label: '码率', width: 120, group: 'basic' },
  { key: 'size', label: '文件大小', width: 100, group: 'basic' },
  { key: 'scanned_at', label: '入库时间', width: 160, group: 'basic' },
  { key: 'bit_depth', label: '位深', width: 80, group: 'video' },
  { key: 'hdr_format', label: 'HDR', width: 150, group: 'video' },
  { key: 'scan_type', label: '扫描方式', width: 90, group: 'video' },
  { key: 'color_space', label: '色彩空间', width: 90, group: 'video' },
  { key: 'color_primaries', label: '色域', width: 100, group: 'video' },
  { key: 'transfer_characteristics', label: '传输特性', width: 110, group: 'video' },
  { key: 'chroma_subsampling', label: '色度采样', width: 90, group: 'video' },
  { key: 'format_profile', label: '编码Profile', width: 120, group: 'video' },
  { key: 'video_bit_rate', label: '视频码率', width: 110, group: 'video' },
  { key: 'video_stream_size', label: '视频流大小', width: 110, group: 'video' },
  { key: 'video_language', label: '视频语言', width: 100, group: 'video' },
  { key: 'channels', label: '声道', width: 100, group: 'audio' },
  { key: 'channel_layout', label: '声道布局', width: 120, group: 'audio' },
  { key: 'audio_codec', label: '音频编码', width: 110, group: 'audio' },
  { key: 'audio_bit_rate', label: '音频码率', width: 110, group: 'audio' },
  { key: 'sample_rate', label: '采样率', width: 90, group: 'audio' },
  { key: 'audio_language', label: '音频语言', width: 100, group: 'audio' },
  { key: 'audio_stream_size', label: '音频流大小', width: 110, group: 'audio' },
  { key: 'text_count', label: '字幕数', width: 80, group: 'extra' },
  { key: 'text_languages', label: '字幕语言', width: 150, group: 'extra' },
  { key: 'writing_application', label: '封装工具', width: 150, group: 'extra' },
  { key: 'encoded_library', label: '编码库', width: 150, group: 'extra' },
  { key: 'path', label: '文件路径', width: 300, group: 'extra' },
]
const HISTORY_DEFAULT_KEYS = ['name', 'format', 'resolution', 'codec', 'duration', 'overall_bit_rate', 'scanned_at', 'size']
const historyColKeys = ref<string[]>([...HISTORY_DEFAULT_KEYS])
const historyColPopoverVisible = ref(false)

const GROUP_LABELS: Record<string, string> = { basic: '基本信息', video: '视频流', audio: '音频流', extra: '其他' }
const historyGroupedCols = computed(() => {
  const groups: Record<string, HistoryColDef[]> = { basic: [], video: [], audio: [], extra: [] }
  HISTORY_ALL_COLUMNS.forEach(c => { const g = groups[c.group]; if (g) g.push(c) })
  return groups
})

// 从 detail_json 提取额外字段（Map 缓存，用 id 做键，避免 WeakMap 因对象重建而失效）
const detailCache = new Map<string, any>()
const DETAIL_CACHE_MAX = 200
function getExtraField(row: VideoRecordRow, key: string): string {
  if (!row.detail_json) return '-'
  try {
    let d = detailCache.get(row.id)
    if (d === undefined) {
      d = JSON.parse(row.detail_json)
      if (detailCache.size >= DETAIL_CACHE_MAX) {
        const firstKey = detailCache.keys().next().value
        if (firstKey) detailCache.delete(firstKey)
      }
      detailCache.set(row.id, d)
    }
    const g = d.general ?? {}
    const v = d.videoStreams?.[0] ?? {}
    const a = d.audioStreams?.[0] ?? {}
    const texts: any[] = d.textStreams ?? []
    switch (key) {
      case 'bit_depth': return v.bitDepth ?? '-'
      case 'hdr_format': return v.hdrFormat ?? '-'
      case 'scan_type': return v.scanType ?? '-'
      case 'color_space': return v.colorSpace ?? '-'
      case 'color_primaries': return v.colorPrimaries ?? '-'
      case 'transfer_characteristics': return v.transferCharacteristics ?? '-'
      case 'chroma_subsampling': return v.chromaSubsampling ?? '-'
      case 'format_profile': return v.formatProfile ?? '-'
      case 'video_bit_rate': return v.bitRate ?? '-'
      case 'video_stream_size': return v.streamSize ?? '-'
      case 'video_language': return v.language ?? '-'
      case 'channels': return a.channels ?? '-'
      case 'channel_layout': return a.channelLayout ?? '-'
      case 'audio_codec': return a.codec ?? '-'
      case 'audio_bit_rate': return a.bitRate ?? '-'
      case 'sample_rate': return a.sampleRate ?? '-'
      case 'audio_language': return a.language ?? '-'
      case 'audio_stream_size': return a.streamSize ?? '-'
      case 'text_count': return texts.length ? String(texts.length) : '-'
      case 'text_languages': {
        const langs = [...new Set(texts.map((t: any) => t.language).filter(Boolean))]
        return langs.length ? langs.join(', ') : '-'
      }
      case 'writing_application': return g.writingApplication ?? '-'
      case 'encoded_library': return v.encodedLibrary || g.encodedLibrary || '-'
      case 'path': return row.path ?? '-'
      default: return '-'
    }
  } catch { return '-' }
}
const searchQuery = ref('')
const viewMode = ref<'card' | 'table'>('table')

// 全空列处理模式：right=靠右显示, hide=隐藏
const emptyColMode = ref<'right' | 'hide'>('right')
// 高亮差异开关
const highlightDiff = ref(false)

// ==================== 列头筛选 ====================
// 每列的筛选条件：prop -> Set of selected values (空Set=不过滤)
const columnFilters = ref<Map<string, Set<string>>>(new Map())
const filterPopoverCol = ref<string | null>(null)
const filterSearch = ref('')

// 预计算所有列的唯一值（单次遍历，数据变化时才重算）
const allColumnUniqueValues = computed(() => {
  const rows = flatTableData.value
  const map = new Map<string, Set<string>>()
  for (const row of rows) {
    for (const col of ALL_COLUMNS) {
      const v = String((row as any)[col.prop] ?? '')
      if (v && v !== '-' && v !== '0') {
        if (!map.has(col.prop)) map.set(col.prop, new Set())
        map.get(col.prop)!.add(v)
      }
    }
  }
  const result = new Map<string, string[]>()
  for (const [key, set] of map) {
    result.set(key, [...set].sort())
  }
  return result
})

// 当前列筛选弹窗中的值列表（带搜索过滤）
const filterValueList = computed(() => {
  const col = filterPopoverCol.value
  if (!col) return []
  const all = allColumnUniqueValues.value.get(col) ?? []
  const q = filterSearch.value.toLowerCase().trim()
  if (!q) return all
  return all.filter(v => v.toLowerCase().includes(q))
})

// 切换某列某值的筛选状态
const toggleFilterValue = (prop: string, value: string) => {
  const filters = columnFilters.value
  if (!filters.has(prop)) {
    filters.set(prop, new Set())
  }
  const set = filters.get(prop)!
  if (set.has(value)) {
    set.delete(value)
  } else {
    set.add(value)
  }
  // 触发响应式
  columnFilters.value = new Map(filters)
}

// 全选某列
const selectAllFilter = (prop: string) => {
  const filters = columnFilters.value
  filters.set(prop, new Set())
  columnFilters.value = new Map(filters)
}

// 清空某列筛选
const clearFilter = (prop: string) => {
  const filters = columnFilters.value
  filters.delete(prop)
  columnFilters.value = new Map(filters)
}

// 某列是否有激活的筛选
const isFilterActive = (prop: string): boolean => {
  const f = columnFilters.value.get(prop)
  return !!f && f.size > 0
}

// 打开筛选弹窗
const openFilterPopover = (prop: string) => {
  if (filterPopoverCol.value === prop) {
    filterPopoverCol.value = null
  } else {
    filterPopoverCol.value = prop
    filterSearch.value = ''
  }
}

// 筛选后的数据（应用所有列筛选）
const applyColumnFilters = (rows: ReturnType<typeof extractRow>[]) => {
  const filters = columnFilters.value
  if (filters.size === 0) return rows
  return rows.filter(row => {
    for (const [prop, selected] of filters) {
      if (selected.size === 0) continue // 空Set=不过滤
      const val = String((row as any)[prop] ?? '')
      if (!selected.has(val)) return false
    }
    return true
  })
}

const importProgress = reactive({
  active: false,
  percent: 0,
})

// 表格排序状态
const sortState = ref<{ prop: string | null; order: 'ascending' | 'descending' | null }>({ prop: null, order: null })

const compareByOrder = (a: number, b: number, order: 'ascending' | 'descending' | null) => {
  if (!order) return 0
  return order === 'ascending' ? a - b : b - a
}

// 归类功能
type GroupByOption = 'none' | 'resolution' | 'resolutionTier' | 'orientation' | 'codec' | 'hdr' | 'bitDepth' | 'format' | 'frameRate'
const groupBy = ref<GroupByOption>('none')

const GROUP_OPTIONS = computed(() => [
  { label: t('不归类'), value: 'none' as GroupByOption },
  { label: t('按分辨率'), value: 'resolution' as GroupByOption },
  { label: t('分辨率等级'), value: 'resolutionTier' as GroupByOption },
  { label: t('按方向'), value: 'orientation' as GroupByOption },
  { label: t('按编码'), value: 'codec' as GroupByOption },
  { label: t('按HDR'), value: 'hdr' as GroupByOption },
  { label: t('按位深'), value: 'bitDepth' as GroupByOption },
  { label: t('按封装格式'), value: 'format' as GroupByOption },
  { label: t('按帧率'), value: 'frameRate' as GroupByOption },
])

// 分辨率等级（基于宽度）
const getResolutionTier = (w: number): string => {
  if (w <= 0) return '未知'
  if (w < 854) return '360p'
  if (w < 1280) return '480p'
  if (w < 1920) return '720p'
  if (w < 2560) return '1080p'
  if (w < 3840) return '2K'
  if (w < 7680) return '4K'
  return '8K'
}

// 音轨摘要（入门级）：编码 + 声道 + 采样率
const buildAudioSummary = (item: VideoInfoItem): string => {
  const streams = item.detail?.audioStreams
  if (!streams || streams.length === 0) return '-'
  const first = streams[0]!
  const codec = first.codec || '?'
  const ch = parseChannels(first.channels)
  const sr = parseSampleRate(first.sampleRate)
  return [codec, ch, sr].filter(Boolean).join(' ')
}

// 字幕摘要（入门级）：优先显示语言，无语言时显示格式
const buildTextSummary = (item: VideoInfoItem): string => {
  const streams = item.detail?.textStreams
  if (!streams || streams.length === 0) return '-'
  return streams.map(s => {
    if (s.language) return translateLang(s.language)
    if (s.title) return s.title
    return s.format || '?'
  }).join(' / ')
}

// 音轨详情（专业级）：编码 + 声道 + 采样率 + 码率 + 语言
const buildAudioDetail = (item: VideoInfoItem): string => {
  const streams = item.detail?.audioStreams
  if (!streams || streams.length === 0) return '-'
  return streams.map((s, i) => {
    const codec = s.codec || '?'
    const ch = parseChannels(s.channels)
    const sr = parseSampleRate(s.sampleRate)
    const br = parseBitRate(s.bitRate)
    const lang = s.language ? `[${translateLang(s.language)}]` : ''
    return `#${i + 1} ${[codec, ch, sr, br, lang].filter(Boolean).join(' ')}`
  }).join('  ')
}

// 字幕详情（专业级）
const buildTextDetail = (item: VideoInfoItem): string => {
  const streams = item.detail?.textStreams
  if (!streams || streams.length === 0) return '-'
  return streams.map((s, i) => {
    const fmt = s.format || '?'
    const lang = s.language ? `[${translateLang(s.language)}]` : ''
    return `#${i + 1} ${fmt} ${lang}`.trim()
  }).join('  ')
}

// ==================== MediaInfo 字段解析工具 ====================
// 从 MediaInfo 原始字符串中提取第一个数字（支持整数/小数）
const parseFirstNum = (s: string): number => {
  if (!s) return 0
  const m = s.match(/[\d]+(?:\.[\d]+)?/)
  return m ? parseFloat(m[0]) : 0
}

// 解析声道数："6 channels" → "6ch", "2 / 6 channels" → "2ch", "6" → "6ch"
const parseChannels = (s: string): string => {
  if (!s) return ''
  const n = parseFirstNum(s)
  return n > 0 ? `${Math.round(n)}ch` : ''
}

// 解析采样率："48000" → "48kHz", "48000 Hz" → "48kHz", "48.0 kHz" → "48kHz"
const parseSampleRate = (s: string): string => {
  if (!s) return ''
  const lower = s.toLowerCase()
  // 已经是 kHz 格式
  if (lower.includes('khz')) {
    const n = parseFirstNum(s)
    return n > 0 ? `${Math.round(n)}kHz` : ''
  }
  // Hz 格式或纯数字（默认 Hz）
  const n = parseFirstNum(s)
  if (n <= 0) return ''
  // 如果数字大于1000，认为是 Hz，转换为 kHz
  if (n >= 1000) return `${Math.round(n / 1000)}kHz`
  // 小于1000，可能已经是 kHz
  return `${Math.round(n)}kHz`
}

// 解析码率："562000" → "562kbps", "562 kb/s" → "562kbps", "562 kbps" → "562kbps"
const parseBitRate = (s: string): string => {
  if (!s) return ''
  const lower = s.toLowerCase()
  // 已经是 kbps / kb/s 格式
  if (lower.includes('kb') || lower.includes('kbit')) {
    const n = parseFirstNum(s)
    return n > 0 ? `${Math.round(n)}kbps` : ''
  }
  // bps 格式或纯数字（默认 bps）
  const n = parseFirstNum(s)
  if (n <= 0) return ''
  // 如果数字大于10000，认为是 bps，转换为 kbps
  if (n >= 10000) return `${Math.round(n / 1000)}kbps`
  // 小于10000，可能已经是 kbps
  return `${Math.round(n)}kbps`
}

const parseBitRateMbps = (s: string): number => {
  if (!s) return 0
  const lower = s.toLowerCase()
  const n = parseFirstNum(s)
  if (n <= 0) return 0
  if (lower.includes('mb')) return n
  if (lower.includes('kb') || lower.includes('kbit')) return n / 1000
  if (n >= 100_000) return n / 1_000_000
  if (n >= 1000) return n / 1000
  return n
}

const isCommonFrameRate = (value: number): boolean => {
  if (value <= 0) return true
  return [23.976, 24, 25, 29.97, 30, 50, 59.94, 60, 120].some((common) => Math.abs(value - common) < 0.12)
}

const getWorstSeverity = (issues: HealthIssue[]): HealthSeverity | 'success' => {
  if (issues.some((issue) => issue.severity === 'danger')) return 'danger'
  if (issues.some((issue) => issue.severity === 'warning')) return 'warning'
  if (issues.some((issue) => issue.severity === 'info')) return 'info'
  return 'success'
}

const getSeverityTagType = (severity: HealthSeverity | 'success') => {
  if (severity === 'danger') return 'danger'
  if (severity === 'warning') return 'warning'
  if (severity === 'info') return 'info'
  return 'success'
}

const analyzeHealth = (item: VideoInfoItem): HealthIssue[] => {
  const issues: HealthIssue[] = []
  if (item.status !== 'success') {
    issues.push({ severity: 'danger', label: '读取失败', detail: item.reason || '元数据读取失败' })
    return issues
  }

  const detail = item.detail
  if (!detail) {
    issues.push({ severity: 'danger', label: '缺少详情', detail: '没有拿到 MediaInfo 详情数据' })
    return issues
  }

  const video = detail.videoStreams?.[0]
  const audioStreams = detail.audioStreams || []
  const textStreams = detail.textStreams || []
  if (!video) {
    issues.push({ severity: 'danger', label: '无视频流', detail: '文件没有可识别的视频流' })
    return issues
  }

  const bitrateMbps = parseBitRateMbps(video.bitRate || detail.general?.overallBitRate || '')
  const width = video.width || 0
  const frameRate = parseFirstNum(video.frameRate || '')
  const bitDepth = parseFirstNum(video.bitDepth || '')
  const durationMs = video.durationMs || detail.general?.durationMs || 0

  if (!durationMs) issues.push({ severity: 'warning', label: '时长缺失', detail: '无法识别有效时长' })
  if (!audioStreams.length) issues.push({ severity: 'warning', label: '无音轨', detail: '未检测到音频流' })
  if (!textStreams.length) issues.push({ severity: 'info', label: '无字幕', detail: '未检测到字幕流' })
  if (width >= 3840 && bitrateMbps > 0 && bitrateMbps < 12) {
    issues.push({ severity: 'warning', label: '4K低码率', detail: `4K 视频码率约 ${bitrateMbps.toFixed(2)} Mbps` })
  } else if (width >= 1920 && bitrateMbps > 0 && bitrateMbps < 3) {
    issues.push({ severity: 'warning', label: '1080p低码率', detail: `1080p 视频码率约 ${bitrateMbps.toFixed(2)} Mbps` })
  }
  if (frameRate > 0 && !isCommonFrameRate(frameRate)) {
    issues.push({ severity: 'info', label: '非常规帧率', detail: `帧率为 ${frameRate}` })
  }
  if (video.hdrFormat && video.hdrFormat !== '-' && bitDepth > 0 && bitDepth < 10) {
    issues.push({ severity: 'warning', label: 'HDR位深偏低', detail: `HDR 视频位深为 ${video.bitDepth}` })
  }
  if ((detail.videoStreams?.length || 0) > 1) {
    issues.push({ severity: 'info', label: '多视频流', detail: `检测到 ${detail.videoStreams.length} 条视频流` })
  }

  return issues
}

const healthIssueMap = computed(() => new Map(items.value.map((item) => [item.id, analyzeHealth(item)])))

const healthSummary = computed(() => {
  let danger = 0
  let warning = 0
  let info = 0
  for (const issues of healthIssueMap.value.values()) {
    issues.forEach((issue) => {
      if (issue.severity === 'danger') danger++
      else if (issue.severity === 'warning') warning++
      else info++
    })
  }
  return {
    danger,
    warning,
    info,
    totalIssues: danger + warning + info,
    cleanFiles: items.value.filter((item) => (healthIssueMap.value.get(item.id) || []).length === 0).length
  }
})

const getHealthIssues = (id: number) => healthIssueMap.value.get(id) || []

// 从 VideoInfoItem 提取表格行
const extractRow = (item: VideoInfoItem) => {
  const g = item.detail?.general
  const v = item.detail?.videoStreams?.[0]
  const w = v?.width || 0
  const h = v?.height || 0
  const healthIssues = analyzeHealth(item)
  const healthSeverity = getWorstSeverity(healthIssues)
  return {
    id: item.id,
    name: item.name,
    path: item.path,
    size: item.size,
    status: item.status,
    format: g?.format || '-',
    duration: g?.duration || '-',
    durationMs: g?.durationMs || 0,
    resolution: v ? `${w}×${h}` : '-',
    width: w,
    height: h,
    pixelCount: v ? w * h : 0,
    resolutionTier: getResolutionTier(w),
    orientation: w > 0 && h > 0 ? (w > h ? '横屏' : w < h ? '竖屏' : '正方形') : '未知',
    codec: v?.codec || '-',
    bitrate: g?.overallBitRate || '-',
    bitrateNum: v?.bitRate ? parseFloat(v.bitRate) : 0,
    frameRate: v?.frameRate || '-',
    frameRateNum: v?.frameRate ? parseFloat(v.frameRate) : 0,
    hdrFormat: v?.hdrFormat || '-',
    bitDepth: v?.bitDepth || '-',
    chromaSubsampling: v?.chromaSubsampling || '-',
    colorSpace: v?.colorSpace || '-',
    colorPrimaries: v?.colorPrimaries || '-',
    transferCharacteristics: v?.transferCharacteristics || '-',
    formatProfile: v?.formatProfile || '-',
    cabac: v?.cabac || '-',
    refFrames: v?.formatSettingsRefFrames || '-',
    encodedLibrary: v?.encodedLibrary || '-',
    codecId: v?.codecId || '-',
    videoCount: item.detail?.videoStreams?.length || 0,
    audioCount: item.detail?.audioStreams?.length || 0,
    textCount: item.detail?.textStreams?.length || 0,
    healthIssues,
    healthIssueCount: healthIssues.length,
    healthSeverity,
    healthLabel: healthIssues.length ? healthIssues.map((issue) => issue.label).join(' / ') : '正常',
    audioSummary: buildAudioSummary(item),
    textSummary: buildTextSummary(item),
    audioDetail: buildAudioDetail(item),
    textDetail: buildTextDetail(item),
  }
}

// 排序通用函数
const sortRows = (rows: ReturnType<typeof extractRow>[]) => {
  const { prop, order } = sortState.value
  if (!prop || !order) return rows
  return [...rows].sort((a: any, b: any) => {
    if (prop === 'name') return order === 'ascending' ? a.name.localeCompare(b.name) : b.name.localeCompare(a.name)
    if (prop === 'size') return compareByOrder(a.size, b.size, order)
    if (prop === 'durationMs') return compareByOrder(a.durationMs, b.durationMs, order)
    if (prop === 'pixelCount') return compareByOrder(a.pixelCount, b.pixelCount, order)
    if (prop === 'width') return compareByOrder(a.width, b.width, order)
    if (prop === 'height') return compareByOrder(a.height, b.height, order)
    if (prop === 'bitrateNum') return compareByOrder(a.bitrateNum, b.bitrateNum, order)
    if (prop === 'frameRateNum') return compareByOrder(a.frameRateNum, b.frameRateNum, order)
    if (prop === 'healthIssueCount') return compareByOrder(a.healthIssueCount, b.healthIssueCount, order)
    if (prop === 'format') return order === 'ascending' ? a.format.localeCompare(b.format) : b.format.localeCompare(a.format)
    if (prop === 'codec') return order === 'ascending' ? a.codec.localeCompare(b.codec) : b.codec.localeCompare(a.codec)
    return 0
  })
}

// 预过滤数据（搜索 + 排序，不含列筛选）—— columnAnalysis 依赖此层，避免列筛选触发重算
const preFilterTableData = computed(() => {
  const rows = filteredItems.value.map(extractRow)
  return sortRows(rows)
})

// 最终平铺数据（应用列筛选）
const flatTableData = computed(() => applyColumnFilters(preFilterTableData.value))

// 归类后的分组数据
interface TableGroup {
  label: string
  count: number
  rows: ReturnType<typeof extractRow>[]
}

const groupedTableData = computed<TableGroup[]>(() => {
  if (groupBy.value === 'none') return []
  const allRows = applyColumnFilters(preFilterTableData.value)
  const map = new Map<string, ReturnType<typeof extractRow>[]>()

  for (const row of allRows) {
    let key: string
    switch (groupBy.value) {
      case 'resolution': key = row.resolution || '未知'; break
      case 'resolutionTier': key = row.resolutionTier || '未知'; break
      case 'orientation': key = row.orientation || '未知'; break
      case 'codec': key = row.codec || '未知'; break
      case 'hdr': key = (row.hdrFormat && row.hdrFormat !== '-') ? row.hdrFormat : 'SDR'; break
      case 'bitDepth': key = (row.bitDepth && row.bitDepth !== '-') ? `${row.bitDepth}bit` : '未知'; break
      case 'format': key = row.format || '未知'; break
      case 'frameRate': key = row.frameRate || '未知'; break
      default: key = '未知'
    }
    if (!map.has(key)) map.set(key, [])
    map.get(key)!.push(row)
  }

  return Array.from(map.entries())
    .map(([label, rows]) => ({ label, count: rows.length, rows: sortRows(rows) }))
    .sort((a, b) => b.count - a.count)
})

const handleSortChange = (payload: { prop: string | null; order: 'ascending' | 'descending' | null }) => {
  sortState.value = payload
}

const LEVEL_OPTIONS = computed(() => [
  { label: t('大众'), value: 'public' as DisplayLevel },
  { label: t('入门'), value: 'beginner' as DisplayLevel },
  { label: t('进阶'), value: 'advanced' as DisplayLevel },
  { label: t('专业'), value: 'professional' as DisplayLevel },
])

const LEVEL_DESC: Record<DisplayLevel, string> = {
  public: '基础文件信息、分辨率、帧率、流数量',
  beginner: '编码格式、码率、帧率、音轨/字幕数量',
  advanced: '位深、HDR、色度抽样、色域、音轨/字幕规格',
  professional: 'CABAC、编码库、编码档次、音轨/字幕详细参数',
}

const LEVEL_ORDER: DisplayLevel[] = ['public', 'beginner', 'advanced', 'professional']
const levelIndex = computed(() => LEVEL_ORDER.indexOf(level.value))
const show = (minLevel: DisplayLevel, maxLevel?: DisplayLevel) => {
  const idx = levelIndex.value
  if (idx < LEVEL_ORDER.indexOf(minLevel)) return false
  if (maxLevel && idx > LEVEL_ORDER.indexOf(maxLevel)) return false
  return true
}

// ==================== 列定义系统 ====================
interface ColumnDef {
  prop: string
  label: string
  width: number
  minLevel: DisplayLevel
  maxLevel?: DisplayLevel  // 超过此等级不再显示
  align?: string
  sortable?: boolean
  slot?: string  // 自定义渲染插槽名
  resizable?: boolean
}

const ALL_COLUMNS: ColumnDef[] = [
  { prop: 'healthIssueCount', label: '体检', width: 130, minLevel: 'public', align: 'center', sortable: true, slot: 'health' },
  { prop: 'format', label: '封装格式', width: 100, minLevel: 'public', align: 'center', sortable: true },
  { prop: 'durationMs', label: '时长', width: 90, minLevel: 'public', align: 'center', sortable: true, slot: 'duration' },
  { prop: 'pixelCount', label: '分辨率', width: 120, minLevel: 'public', align: 'center', sortable: true, slot: 'resolution' },
  { prop: 'codec', label: '视频编码', width: 100, minLevel: 'beginner', align: 'center', sortable: true },
  { prop: 'bitrateNum', label: '码率', width: 120, minLevel: 'beginner', align: 'center', sortable: true, slot: 'bitrate' },
  { prop: 'frameRateNum', label: '帧率', width: 80, minLevel: 'beginner', align: 'center', sortable: true, slot: 'frameRate' },
  { prop: 'audioCount', label: '音轨', width: 70, minLevel: 'beginner', align: 'center', slot: 'audioCount' },
  { prop: 'textCount', label: '字幕', width: 70, minLevel: 'beginner', align: 'center', slot: 'textCount' },
  { prop: 'bitDepth', label: '位深', width: 70, minLevel: 'advanced', align: 'center' },
  { prop: 'hdrFormat', label: 'HDR', width: 100, minLevel: 'advanced', align: 'center', slot: 'hdr' },
  { prop: 'chromaSubsampling', label: '色度', width: 80, minLevel: 'advanced', align: 'center' },
  { prop: 'audioSummary', label: '音轨规格', width: 180, minLevel: 'advanced', maxLevel: 'advanced', align: 'center', slot: 'audioSummary' },
  { prop: 'textSummary', label: '字幕规格', width: 130, minLevel: 'advanced', maxLevel: 'advanced', align: 'center', slot: 'textSummary' },
  { prop: 'colorSpace', label: '色彩空间', width: 100, minLevel: 'advanced', align: 'center' },
  { prop: 'colorPrimaries', label: '色域', width: 100, minLevel: 'advanced', align: 'center', slot: 'tooltip' },
  { prop: 'formatProfile', label: '编码档次', width: 120, minLevel: 'professional', align: 'center', slot: 'tooltip' },
  { prop: 'cabac', label: 'CABAC', width: 80, minLevel: 'professional', align: 'center' },
  { prop: 'refFrames', label: '参考帧', width: 80, minLevel: 'professional', align: 'center' },
  { prop: 'encodedLibrary', label: '编码库', width: 140, minLevel: 'professional', align: 'center', slot: 'tooltip' },
  { prop: 'codecId', label: '编码标识', width: 130, minLevel: 'professional', align: 'center', slot: 'tooltip' },
  { prop: 'audioDetail', label: '音轨详情', width: 220, minLevel: 'professional', align: 'center', slot: 'audioDetail' },
  { prop: 'textDetail', label: '字幕详情', width: 160, minLevel: 'professional', align: 'center', slot: 'textDetail' },
]

// 数据列分析：检测全空列和全同列（依赖 preFilterTableData，不因列筛选变化而重算）
const columnAnalysis = computed(() => {
  const rows = preFilterTableData.value
  if (rows.length === 0) return { empty: new Set<string>(), same: new Set<string>(), sameValues: new Map<string, string>() }

  const empty = new Set<string>()
  const same = new Set<string>()
  const sameValues = new Map<string, string>()

  for (const col of ALL_COLUMNS) {
    const values = rows.map(r => (r as any)[col.prop] as string)
    const nonEmpty = values.filter(v => v && v !== '-' && v !== '0' && v !== '未知')

    if (nonEmpty.length === 0) {
      empty.add(col.prop)
    } else if (nonEmpty.length > 0) {
      const unique = new Set(nonEmpty)
      if (unique.size === 1) {
        same.add(col.prop)
        sameValues.set(col.prop, nonEmpty[0]!)
      }
    }
  }

  return { empty, same, sameValues }
})

// 动态排序后的可见列
const visibleColumns = computed<ColumnDef[]>(() => {
  const { empty, same } = columnAnalysis.value

  // 筛选当前等级可见的列
  const levelCols = ALL_COLUMNS.filter(c => show(c.minLevel, c.maxLevel))

  // 分类：正常列、全同列、全空列
  const normal: ColumnDef[] = []
  const sameValue: ColumnDef[] = []
  const emptyValue: ColumnDef[] = []

  for (const col of levelCols) {
    if (empty.has(col.prop)) {
      emptyValue.push(col)
    } else if (same.has(col.prop)) {
      sameValue.push(col)
    } else {
      normal.push(col)
    }
  }

  // 组装：正常 → 全同 → (全空或隐藏)
  const result = [...normal, ...sameValue]
  if (emptyColMode.value === 'right') {
    result.push(...emptyValue)
  }
  // hide 模式下不添加全空列

  return result
})

// 动态列宽计算
const tableContainerWidth = ref(0)
const tableContainerRef = ref<HTMLElement | null>(null)

// 动态表格高度：自适应窗口
const windowHeight = ref(window.innerHeight)
const tableMaxHeight = computed(() => {
  // 减去顶部工具栏(~120)、等级栏(~60)、统计栏(~40)、间距(~80)
  const overhead = 300
  return Math.max(400, windowHeight.value - overhead)
})

let windowResizeHandler: (() => void) | null = null

// 固定列宽度：序号(60) + 文件名(动态) + 文件大小(100) + 流(100)
const FIXED_BASE_WIDTH = 60 + 100 + 100 // 序号 + 大小 + 流 = 260
const SCROLLBAR_BUFFER = 40

const NAME_COL_MIN = 150
const NAME_COL_MAX = 800

const nameColWidth = computed(() => {
  const containerW = tableContainerWidth.value || 900
  const dataColsW = visibleColumns.value.reduce((sum, c) => sum + c.width, 0)
  const totalFixed = FIXED_BASE_WIDTH + dataColsW
  const available = containerW - totalFixed - SCROLLBAR_BUFFER
  return Math.max(NAME_COL_MIN, Math.min(NAME_COL_MAX, available))
})

let resizeObserver: ResizeObserver | null = null

const observeTableContainer = (el: HTMLElement | null) => {
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
  if (el) {
    tableContainerRef.value = el
    tableContainerWidth.value = el.clientWidth
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        tableContainerWidth.value = entry.contentRect.width
      }
    })
    resizeObserver.observe(el)
  }
}

onUnmounted(() => {
  isMounted = false
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
  if (windowResizeHandler) {
    window.removeEventListener('resize', windowResizeHandler)
    windowResizeHandler = null
  }
  if (unlistenProgress) {
    unlistenProgress()
    unlistenProgress = null
  }
})

// 监听窗口大小变化
windowResizeHandler = () => { windowHeight.value = window.innerHeight }
window.addEventListener('resize', windowResizeHandler)

const filteredItems = computed(() => {
  if (!searchQuery.value.trim()) return items.value
  const q = searchQuery.value.toLowerCase()
  return items.value.filter(
    (item) =>
      item.name.toLowerCase().includes(q) ||
      item.path.toLowerCase().includes(q)
  )
})

const successCount = computed(() => items.value.filter((i) => i.status === 'success').length)
const failedCount = computed(() => items.value.filter((i) => i.status !== 'success').length)

const toggleExpand = (id: number) => {
  if (expandedIds.value.has(id)) {
    expandedIds.value.delete(id)
  } else {
    expandedIds.value.add(id)
  }
}

const expandAll = () => {
  filteredItems.value.forEach((item) => expandedIds.value.add(item.id))
}

const collapseAll = () => {
  expandedIds.value.clear()
}

const makeCacheKey = (paths: string[], isRecursive: boolean) =>
  JSON.stringify({ recursive: isRecursive, paths: [...paths].sort() })

const handleImport = async (kind: 'folder' | 'clipboard') => {
  const picked = await pick(kind)
  const paths = (picked || []).filter((p: string) => !/[\*\?\[\]]/.test(p))
  if (!paths || paths.length === 0) return
  const cacheKey = makeCacheKey(paths, recursive.value)
  const cached = videoInfoCache.get(cacheKey)
  if (cached) {
    items.value = cached.items
    importSummary.value = `${t('成功')} ${cached.success}，${t('失败')} ${cached.failed}，${t('共')} ${cached.total} ${t('个视频')}`
    expandedIds.value.clear()
    ElMessage.success(t('已使用本次会话缓存结果'))
    return
  }

  importing.value = true
  importSummary.value = ''
  expandedIds.value.clear()

  try {
    const resp = await importDetailedVideoInfo(paths, recursive.value)
    videoInfoCache.set(cacheKey, resp)
    items.value = resp.items
    importSummary.value = `${t('成功')} ${resp.success}，${t('失败')} ${resp.failed}，${t('共')} ${resp.total} ${t('个视频')}`

    if (resp.failed > 0) {
      ElMessage.warning(importSummary.value)
    } else {
      ElMessage.success(importSummary.value)
    }
  } catch (e: any) {
    ElMessage.error(e?.toString() || t('导入失败'))
  } finally {
    importing.value = false
    importProgress.active = false
    importProgress.percent = 0
  }
}

const handleClear = () => {
  items.value = []
  expandedIds.value.clear()
  importSummary.value = ''
  searchQuery.value = ''
}

// ==================== 数据库入库 ====================
const formatDurationMs = (ms: number): string => {
  const totalSecs = Math.floor(ms / 1000)
  const h = Math.floor(totalSecs / 3600)
  const m = Math.floor((totalSecs % 3600) / 60)
  const s = totalSecs % 60
  return h > 0
    ? `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
    : `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}

const refreshHistoryCount = async () => {
  try {
    historyCount.value = await getRecordCount()
  } catch { historyCount.value = 0 }
}

const handleSaveToDb = async () => {
  if (!items.value.length) return
  saving.value = true
  saveProgress.total = items.value.length
  saveProgress.saved = 0
  saveProgress.phase = 'xml'

  try {
    const xmlMap = new Map<string, string>()
    const successItems = items.value.filter(i => i.status === 'success')

    // 并发获取 XML（限制 4 路并发）
    const CONCURRENCY = 4
    let completed = 0
    const fetchXml = async (item: VideoInfoItem) => {
      try {
        const xml = await getVideoRawXml(item.path)
        if (xml) xmlMap.set(item.path, xml)
      } catch { /* 跳过 */ }
      completed++
      saveProgress.saved = completed
    }
    const queue = [...successItems]
    const workers = Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
      while (queue.length) {
        const item = queue.shift()!
        await fetchXml(item)
      }
    })
    await Promise.allSettled(workers)

    saveProgress.phase = 'db'
    const count = await saveVideoRecords(items.value, xmlMap)

    saveProgress.phase = 'done'
    ElMessage.success(`${t('已保存')} ${count} ${t('条记录到数据库')}`)
    await refreshHistoryCount()
  } catch (e: any) {
    ElMessage.error(`${t('入库失败')}：${e?.message || e}`)
  } finally {
    saving.value = false
  }
}

const openHistory = async () => {
  historyVisible.value = true
  historyLoading.value = true
  historyHasMore.value = true
  try {
    historyRecords.value = await loadVideoRecords(historyPageSize, 0)
    historyHasMore.value = historyRecords.value.length >= historyPageSize
  } catch (e: any) {
    ElMessage.error(`${t('加载历史记录失败')}：${e?.message || e}`)
  } finally {
    historyLoading.value = false
  }
}

const loadMoreHistory = async () => {
  if (historyLoading.value || !historyHasMore.value) return
  historyLoading.value = true
  try {
    const more = await loadVideoRecords(historyPageSize, historyRecords.value.length)
    historyRecords.value = [...historyRecords.value, ...more]
    historyHasMore.value = more.length >= historyPageSize
  } catch (e: any) {
    ElMessage.error(`${t('加载历史记录失败')}：${e?.message || e}`)
  } finally {
    historyLoading.value = false
  }
}

const loadHistoryToView = () => {
  const rows = historySelection.value.length
    ? historyRecords.value.filter(r => historySelection.value.includes(r.id))
    : historyRecords.value

  items.value = rows.map(rowToVideoInfoItem)
  importSummary.value = `${t('从历史记录加载')} ${items.value.length} ${t('条')}`
  expandedIds.value.clear()
  historyVisible.value = false
  ElMessage.success(`${t('已加载')} ${items.value.length} ${t('条历史记录')}`)
}

const deleteSelectedHistory = async () => {
  if (!historySelection.value.length) return
  try {
    await deleteVideoRecords(historySelection.value)
    historyRecords.value = historyRecords.value.filter(r => !historySelection.value.includes(r.id))
    historySelection.value = []
    await refreshHistoryCount()
    ElMessage.success(t('删除成功'))
  } catch (e: any) {
    ElMessage.error(`${t('删除失败')}：${e?.message || e}`)
  }
}

let unlistenProgress: (() => void) | null = null
let isMounted = false

onMounted(() => {
  isMounted = true
  refreshHistoryCount()

  if (!hasTauriRuntime()) return

  // 监听后端进度事件
  listen<{ done: number; total: number; phase: string }>('video_info_import', (event) => {
    const { done, total, phase } = event.payload
    if (phase === 'start') {
      importProgress.active = true
      importProgress.percent = 0
    } else if (phase === 'done') {
      importProgress.percent = 100
      setTimeout(() => {
        importProgress.active = false
      }, 500)
    } else if (total > 0) {
      importProgress.percent = Math.round((done / total) * 100)
    }
  }).then(unlisten => {
    if (isMounted) {
      unlistenProgress = unlisten
    } else {
      unlisten()
    }
  })
})

// ==================== 导出功能 ====================
const csvEscape = (v: string | number) => {
  const s = String(v)
  return s.includes(',') || s.includes('"') || s.includes('\n')
    ? `"${s.replace(/"/g, '""')}"` : s
}

const exportRows = computed(() => preFilterTableData.value)

const exportHeaders = computed(() => [t('序号'), t('文件名'), ...visibleColumns.value.map(c => t(c.label)), t('文件大小'), t('流')])

const exportDataRows = computed(() =>
  exportRows.value.map((row, idx) => {
    const cells: (string | number)[] = [
      idx + 1,
      row.name,
      ...visibleColumns.value.map(c => {
        const v = (row as any)[c.prop]
        if (c.prop === 'healthIssueCount') return row.healthLabel
        return v ?? '-'
      }),
      formatBytes(row.size),
      [
        row.videoCount ? `V${row.videoCount}` : '',
        row.audioCount ? `A${row.audioCount}` : '',
        row.textCount ? `S${row.textCount}` : '',
      ].filter(Boolean).join(' '),
    ]
    return cells
  })
)

const formatIssueText = (issues: HealthIssue[]) => issues.map((issue) => `${issue.label}: ${issue.detail}`).join('; ') || '正常'

const escapeHtml = (value: string | number) =>
  String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')

const buildMarkdownReport = () => {
  const lines = [
    '# 视频体检报告',
    '',
    `生成时间：${new Date().toLocaleString()}`,
    '',
    `- 文件数：${items.value.length}`,
    `- 正常文件：${healthSummary.value.cleanFiles}`,
    `- 严重问题：${healthSummary.value.danger}`,
    `- 警告：${healthSummary.value.warning}`,
    `- 提示：${healthSummary.value.info}`,
    '',
    '| 文件 | 分辨率 | 编码 | 码率 | 帧率 | 体检结果 |',
    '|---|---:|---|---:|---:|---|'
  ]

  exportRows.value.forEach((row) => {
    lines.push(`| ${row.name} | ${row.resolution} | ${row.codec} | ${row.bitrate} | ${row.frameRate} | ${formatIssueText(row.healthIssues)} |`)
  })
  return lines.join('\n')
}

const buildHtmlReport = () => {
  const rows = exportRows.value.map((row) => `
    <tr>
      <td>${escapeHtml(row.name)}</td>
      <td>${escapeHtml(row.resolution)}</td>
      <td>${escapeHtml(row.codec)}</td>
      <td>${escapeHtml(row.bitrate)}</td>
      <td>${escapeHtml(row.frameRate)}</td>
      <td>${escapeHtml(formatIssueText(row.healthIssues))}</td>
    </tr>
  `).join('')
  return `<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <title>视频体检报告</title>
  <style>
    body{font-family:Arial,"Microsoft YaHei",sans-serif;margin:28px;color:#1f2430}
    h1{margin-bottom:8px}
    .meta{color:#667085;margin-bottom:18px}
    .summary{display:flex;gap:10px;flex-wrap:wrap;margin:16px 0}
    .pill{padding:6px 10px;border-radius:8px;background:#f2f4f7}
    table{width:100%;border-collapse:collapse;font-size:13px}
    th,td{border:1px solid #d0d5dd;padding:8px;text-align:left;vertical-align:top}
    th{background:#f8fafc}
  </style>
</head>
<body>
  <h1>视频体检报告</h1>
  <div class="meta">生成时间：${escapeHtml(new Date().toLocaleString())}</div>
  <div class="summary">
    <span class="pill">文件数：${items.value.length}</span>
    <span class="pill">正常：${healthSummary.value.cleanFiles}</span>
    <span class="pill">严重：${healthSummary.value.danger}</span>
    <span class="pill">警告：${healthSummary.value.warning}</span>
    <span class="pill">提示：${healthSummary.value.info}</span>
  </div>
  <table>
    <thead><tr><th>文件</th><th>分辨率</th><th>编码</th><th>码率</th><th>帧率</th><th>体检结果</th></tr></thead>
    <tbody>${rows}</tbody>
  </table>
</body>
</html>`
}

const generateOutputDirName = (format: ExportFormat): string => {
  const extMap: Record<string, string> = { txt: 'txt', xml2json: 'json', xml2md: 'md' }
  const ext = extMap[format] || format
  const now = new Date()
  const pad = (n: number) => String(n).padStart(2, '0')
  const timestamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`
  return `mediainfo_${ext}_${timestamp}`
}

const confirmOutputMode = async () => {
  outputModeDialogVisible.value = false
  const format = pendingExportFormat.value
  if (!format) return
  // 选择文件夹模式时，如果未指定目录则自动生成默认目录名
  if (outputMode.value === 'folder' && !customOutputDir.value) {
    const dirName = generateOutputDirName(format)
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: '选择输出目录',
      defaultPath: dirName
    })
    if (!selected) return
    customOutputDir.value = selected as string
  }
  await doBatchExport(format)
}

const doBatchExport = async (format: ExportFormat) => {
  const extMap: Record<string, string> = { txt: 'txt', xml2json: 'json', xml2md: 'md' }
  const ext = extMap[format]

  const validItems = items.value.filter(i => i.status === 'success' && i.detail)
  if (!validItems.length) {
    ElMessage.warning(t('没有可导出的 MediaInfo 数据'))
    return
  }

  const getExportFn = (path: string) => {
    if (format === 'txt') return getVideoCompleteInfo(path)
    if (format === 'xml2json') return getVideoXmlJson(path)
    return getVideoXmlMarkdown(path)
  }

  try {
    if (outputMode.value === 'same') {
      // 源目录内生成模式：每个视频旁边生成对应文件
      let count = 0
      for (const item of validItems) {
        const result = await getExportFn(item.path)
        if (!result) continue
        const baseName = item.name.replace(/\.[^.]+$/, '')
        const dir = item.path.replace(/[/\\][^/\\]+$/, '')
        const outPath = `${dir}/${baseName}_MediaInfo.${ext}`
        await writeTextExportFile(outPath, result)
        count++
      }
      ElMessage.success(`导出完成（${count} 个文件，已保存到源文件所在目录）`)
    } else {
      // 集中导出模式
      const outDir = customOutputDir.value
      if (!outDir) return

      if (format === 'xml2json') {
        // XML→JSON 合并为一个数组文件
        const results: any[] = []
        for (const item of validItems) {
          const result = await getVideoXmlJson(item.path)
          if (result) {
            try { results.push(JSON.parse(result)) } catch { /* skip */ }
          }
        }
        const outPath = `${outDir}/MediaInfo_ALL.json`
        await writeTextExportFile(outPath, JSON.stringify(results, null, 2))
        ElMessage.success(`导出完成（${results.length} 个文件 → ${outPath}）`)
      } else if (format === 'xml2md') {
        // XML→MD 合并为一个 MD 文件
        const header = `# MediaInfo 批量导出报告\n\n**导出时间:** ${new Date().toLocaleString()}\n**文件数量:** ${validItems.length}\n\n---\n\n`
        const parts: string[] = []
        for (const item of validItems) {
          const result = await getVideoXmlMarkdown(item.path)
          if (result) parts.push(result)
        }
        const outPath = `${outDir}/MediaInfo_ALL.md`
        await writeTextExportFile(outPath, header + parts.join('\n\n---\n\n'))
        ElMessage.success(`导出完成（${parts.length} 个文件 → ${outPath}）`)
      } else {
        // TXT：每个视频单独一个文件
        let count = 0
        for (const item of validItems) {
          const result = await getVideoCompleteInfo(item.path)
          if (!result) continue
          const baseName = item.name.replace(/\.[^.]+$/, '')
          const outPath = `${outDir}/${baseName}_MediaInfo.txt`
          await writeTextExportFile(outPath, result)
          count++
        }
        ElMessage.success(`导出完成（${count} 个文件 → ${outDir}）`)
      }
    }
  } catch (e: any) {
    ElMessage.error(`导出失败：${e?.message || e}`)
  }
}

const exportData = async (format: ExportFormat) => {
  const rows = flatTableData.value
  if (!rows.length) {
    ElMessage.warning(t('没有数据可导出'))
    return
  }

  // 批量格式需要先选择输出模式
  if (format === 'txt' || format === 'xml2json' || format === 'xml2md') {
    pendingExportFormat.value = format
    customOutputDir.value = '' // 重置目录选择
    outputModeDialogVisible.value = true
    return
  }

  // 弹出原生保存对话框（仅 xlsx/csv/markdown/html/json）
  const extensionMap: Record<string, string> = {
    xlsx: 'xlsx',
    csv: 'csv',
    markdown: 'md',
    html: 'html',
    json: 'json'
  }
  const defaultName = `video-health-${Date.now()}.${extensionMap[format]}`
  const filters: Record<string, { name: string; extensions: string[] }[]> = {
    xlsx: [{ name: 'Excel', extensions: ['xlsx'] }],
    csv: [{ name: 'CSV', extensions: ['csv'] }],
    markdown: [{ name: 'Markdown', extensions: ['md'] }],
    html: [{ name: 'HTML', extensions: ['html'] }],
    json: [{ name: 'JSON', extensions: ['json'] }]
  }
  const filePath = await save({
    defaultPath: defaultName,
    filters: filters[format],
  })
  if (!filePath) return // 用户取消

  const headers = exportHeaders.value
  const dataRows = exportDataRows.value

  try {
    if (format === 'csv') {
      const bom = '﻿'
      const csv = bom + [headers.map(csvEscape).join(','), ...dataRows.map(r => r.map(csvEscape).join(','))].join('\n')
      await writeTextExportFile(filePath, csv)
    } else if (format === 'markdown') {
      await writeTextExportFile(filePath, buildMarkdownReport())
    } else if (format === 'html') {
      await writeTextExportFile(filePath, buildHtmlReport())
    } else if (format === 'json') {
      await writeTextExportFile(filePath, JSON.stringify({
        generatedAt: new Date().toISOString(),
        summary: healthSummary.value,
        rows: exportRows.value.map((row) => ({
          ...row,
          healthIssues: row.healthIssues
        }))
      }, null, 2))
    } else {
      const ExcelJS = await import('exceljs')
      const wb = new ExcelJS.Workbook()
      const ws = wb.addWorksheet('Video Info')
      ws.addRow(headers)
      dataRows.forEach(r => ws.addRow(r))

      ws.getRow(1).eachCell(cell => {
        cell.font = { bold: true }
        cell.fill = { type: 'pattern', pattern: 'solid', fgColor: { argb: 'FFE8EEFF' } }
        cell.border = {
          bottom: { style: 'thin', color: { argb: 'FFD0D5DD' } },
        }
      })

      ws.columns.forEach((col, i) => {
        const maxLen = Math.max(
          headers[i]?.length || 8,
          ...dataRows.map(r => String(r[i] || '').length)
        )
        col.width = Math.min(Math.max(maxLen + 2, 8), 40)
      })

      const buf = await wb.xlsx.writeBuffer()
      await writeBinaryExportFile(filePath, Array.from(new Uint8Array(buf)))
    }

    ElMessage.success(`${t('导出成功')}（${rows.length} ${t('行')}）\n${filePath}`)
  } catch (e: any) {
    ElMessage.error(`${t('导出失败')}：${e?.message || e}`)
  }
}
</script>

<template>
  <div class="video-info-view">
    <!-- 顶部工具栏 -->
    <div class="info-toolbar">
      <div class="toolbar-left">
        <h3 class="toolbar-title">{{ t('视频体检') }}</h3>
        <span class="toolbar-sub">{{ t('导入视频文件，检测 MediaInfo 元数据与潜在质量异常') }}</span>
      </div>
      <div class="toolbar-right">
        <el-button :icon="FolderAdd" round :loading="importing" @click="handleImport('folder')">
          {{ t('添加文件夹') }}
        </el-button>
        <el-button :icon="Upload" round plain :loading="importing" @click="handleImport('clipboard')">
          {{ t('粘贴路径导入') }}
        </el-button>
        <label class="switch-field">
          <span class="switch-label">{{ t('递归遍历') }}</span>
          <el-switch v-model="recursive" size="small" />
        </label>
        <el-button v-if="items.length" type="success" :icon="Coin" round :loading="saving" @click="handleSaveToDb">
          {{ t('入库') }}
        </el-button>
        <el-badge :value="historyCount" :hidden="!historyCount" :max="999">
          <el-button round @click="openHistory">{{ t('历史记录') }}</el-button>
        </el-badge>
      </div>
    </div>

    <!-- 进度条 -->
    <div v-if="importProgress.active" class="progress-bar">
      <el-progress :percentage="importProgress.percent" :stroke-width="4" />
    </div>

    <!-- 等级选择 + 统计 -->
    <div v-if="items.length" class="level-bar">
      <div class="level-selector">
        <span class="level-label">{{ t('显示等级') }}</span>
        <el-segmented v-model="level" :options="LEVEL_OPTIONS" size="small" />
      </div>
      <div class="level-desc">{{ t(LEVEL_DESC[level]) }}</div>
      <div class="level-actions">
        <el-tag v-if="healthSummary.danger" size="small" type="danger">{{ t('严重') }} {{ healthSummary.danger }}</el-tag>
        <el-tag v-if="healthSummary.warning" size="small" type="warning">{{ t('警告') }} {{ healthSummary.warning }}</el-tag>
        <el-tag v-if="healthSummary.info" size="small" type="info">{{ t('提示') }} {{ healthSummary.info }}</el-tag>
        <el-tag v-if="!healthSummary.totalIssues" size="small" type="success">{{ t('未发现异常') }}</el-tag>
        <el-select v-if="viewMode === 'table'" v-model="emptyColMode" size="small" style="width: 130px;">
          <el-option :label="t('全空列靠右')" value="right" />
          <el-option :label="t('全空列隐藏')" value="hide" />
        </el-select>
        <label v-if="viewMode === 'table'" class="switch-field">
          <span class="switch-label">{{ t('高亮差异') }}</span>
          <el-switch v-model="highlightDiff" size="small" />
        </label>
        <el-select v-if="viewMode === 'table'" v-model="groupBy" size="small" style="width: 120px;">
          <el-option v-for="opt in GROUP_OPTIONS" :key="opt.value" :label="opt.label" :value="opt.value" />
        </el-select>
        <el-input
          v-model="searchQuery"
          size="small"
          :placeholder="t('搜索文件名...')"
          clearable
          style="width: 200px"
        />
        <el-button-group size="small">
          <el-button :type="viewMode === 'card' ? 'primary' : ''" :icon="Grid" @click="viewMode = 'card'" />
          <el-button :type="viewMode === 'table' ? 'primary' : ''" :icon="List" @click="viewMode = 'table'" />
        </el-button-group>
        <el-button v-if="viewMode === 'card'" size="small" type="primary" plain @click="expandAll">{{ t('全部展开') }}</el-button>
        <el-button v-if="viewMode === 'card'" size="small" type="primary" plain @click="collapseAll">{{ t('全部收起') }}</el-button>
        <el-dropdown v-if="viewMode === 'table'" trigger="click" @command="(cmd: any) => exportData(cmd as ExportFormat)">
          <el-button size="small" type="primary" plain :icon="Download">{{ t('导出') }}</el-button>
          <template #dropdown>
            <el-dropdown-menu>
              <el-dropdown-item command="xlsx">XLSX</el-dropdown-item>
              <el-dropdown-item command="csv">CSV</el-dropdown-item>
              <el-dropdown-item command="markdown">Markdown {{ t('报告') }}</el-dropdown-item>
              <el-dropdown-item command="html">HTML {{ t('报告') }}</el-dropdown-item>
              <el-dropdown-item command="json">JSON</el-dropdown-item>
              <el-dropdown-item divided command="txt">TXT {{ t('完整') }}</el-dropdown-item>
              <el-dropdown-item command="xml2json">XML→JSON</el-dropdown-item>
              <el-dropdown-item command="xml2md">XML→MD</el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
        <el-button size="small" type="danger" plain @click="handleClear">{{ t('清空') }}</el-button>
      </div>
    </div>

    <!-- 导入统计 -->
    <div v-if="importSummary" class="summary-bar">
      <el-tag type="success" size="small">{{ t('成功') }} {{ successCount }}</el-tag>
      <el-tag v-if="failedCount" type="danger" size="small">{{ t('失败') }} {{ failedCount }}</el-tag>
      <span class="summary-text">{{ importSummary }}</span>
    </div>

    <!-- 卡片视图 -->
    <div v-if="viewMode === 'card' && items.length" class="card-list">
      <div v-for="item in filteredItems" :key="item.id" class="health-card-wrapper">
        <div v-if="getHealthIssues(item.id).length" class="health-issues-row">
          <el-tag
            v-for="issue in getHealthIssues(item.id)"
            :key="`${issue.label}-${issue.detail}`"
            size="small"
            :type="getSeverityTagType(issue.severity)"
          >
            {{ t(issue.label) }}
          </el-tag>
        </div>
        <VideoInfoDetailCard
          :item="item"
          :level="level"
          :expanded="expandedIds.has(item.id)"
          @toggle="toggleExpand(item.id)"
        />
      </div>
    </div>

    <!-- 列筛选弹窗（全局，不受 overflow 限制） -->
    <div v-if="filterPopoverCol" class="filter-popover" @click.self="filterPopoverCol = null">
      <div class="filter-panel">
        <div class="filter-title">
          <span>{{ t(visibleColumns.find(c => c.prop === filterPopoverCol)?.label || '') }} {{ t('筛选') }}</span>
          <el-button size="small" text @click="filterPopoverCol = null">&times;</el-button>
        </div>
        <el-input v-model="filterSearch" size="small" :placeholder="t('搜索...')" clearable class="filter-search" />
        <div class="filter-actions">
          <el-button size="small" text type="primary" @click="selectAllFilter(filterPopoverCol!)">{{ t('全选') }}</el-button>
          <el-button size="small" text @click="clearFilter(filterPopoverCol!)">{{ t('清空') }}</el-button>
        </div>
        <el-checkbox-group :model-value="[...(columnFilters.get(filterPopoverCol!) || [])]" class="filter-list">
          <el-checkbox
            v-for="val in filterValueList"
            :key="val"
            :label="val"
            :value="val"
            @change="toggleFilterValue(filterPopoverCol!, val)"
          >{{ val }}</el-checkbox>
        </el-checkbox-group>
      </div>
    </div>

    <!-- 表格视图：无归类 -->
    <div v-else-if="viewMode === 'table' && groupBy === 'none'" class="table-wrapper" :ref="(el: any) => observeTableContainer(el?.$el || el)">
      <el-table
        :data="flatTableData"
        size="small"
        class="info-table"
        row-key="id"
        :max-height="tableMaxHeight"
        border
        @sort-change="handleSortChange"
      >
        <!-- 序号列（成功行绿色） -->
        <el-table-column width="60" align="center" resizable>
          <template #header><span>{{ t('序号') }}</span></template>
          <template #default="{ $index, row }">
            <span :class="{ 'idx-success': row.status === 'success', 'idx-error': row.status !== 'success' }">{{ $index + 1 }}</span>
          </template>
        </el-table-column>

        <!-- 文件名列 -->
        <el-table-column prop="name" :label="t('文件名')" :width="nameColWidth" sortable="custom" resizable>
          <template #header>
            <span class="col-header-text">{{ t('文件名') }}</span>
            <el-icon class="col-filter-btn" :class="{ 'filter-active': isFilterActive('name') }" @click.stop="openFilterPopover('name')"><Filter /></el-icon>
          </template>
          <template #default="{ row }">
            <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('name') }">{{ row.name }}</span>
          </template>
        </el-table-column>

        <!-- 动态数据列 -->
        <el-table-column
          v-for="col in visibleColumns"
          :key="col.prop"
          :prop="col.prop"
          :width="col.width"
          :align="col.align || 'center'"
          :sortable="col.sortable ? 'custom' : undefined"
          resizable
        >
          <template #header>
            <span class="col-header-text">{{ t(col.label) }}</span>
            <el-icon class="col-filter-btn" :class="{ 'filter-active': isFilterActive(col.prop) }" @click.stop="openFilterPopover(col.prop)"><Filter /></el-icon>
          </template>
          <template #default="{ row }">
            <template v-if="col.slot === 'health'">
              <el-popover placement="top" trigger="hover" width="280">
                <template #reference>
                  <el-tag size="small" :type="getSeverityTagType(row.healthSeverity)">
                    {{ row.healthLabel }}
                  </el-tag>
                </template>
                <div class="health-popover">
                  <p v-if="!row.healthIssues.length">{{ t('未发现异常') }}</p>
                  <div v-for="issue in row.healthIssues" :key="`${issue.label}-${issue.detail}`" class="health-popover-item">
                    <el-tag size="small" :type="getSeverityTagType(issue.severity)">{{ t(issue.label) }}</el-tag>
                    <span>{{ issue.detail }}</span>
                  </div>
                </div>
              </el-popover>
            </template>
            <template v-else-if="col.slot === 'duration'">
              <span class="cell-wrap">{{ row.duration }}</span>
            </template>
            <template v-else-if="col.slot === 'resolution'">
              <span class="cell-wrap">{{ row.resolution }}</span>
            </template>
            <template v-else-if="col.slot === 'bitrate'">
              <span class="cell-wrap">{{ row.bitrate }}</span>
            </template>
            <template v-else-if="col.slot === 'frameRate'">
              <span class="cell-wrap">{{ row.frameRate }}</span>
            </template>
            <template v-else-if="col.slot === 'hdr'">
              <el-tag v-if="row.hdrFormat && row.hdrFormat !== '-'" size="small" type="warning" class="cell-wrap">{{ row.hdrFormat }}</el-tag>
              <span v-else>-</span>
            </template>
            <template v-else-if="col.slot === 'audioCount'">
              <span v-if="row.audioCount" class="stream-tag audio">A{{ row.audioCount }}</span>
              <span v-else>-</span>
            </template>
            <template v-else-if="col.slot === 'textCount'">
              <span v-if="row.textCount" class="stream-tag text">S{{ row.textCount }}</span>
              <span v-else>-</span>
            </template>
            <template v-else-if="col.slot === 'audioSummary'">
              <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('audioSummary') }">{{ row.audioSummary }}</span>
            </template>
            <template v-else-if="col.slot === 'textSummary'">
              <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('textSummary') }">{{ row.textSummary }}</span>
            </template>
            <template v-else-if="col.slot === 'audioDetail'">
              <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('audioDetail') }">{{ row.audioDetail }}</span>
            </template>
            <template v-else-if="col.slot === 'textDetail'">
              <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('textDetail') }">{{ row.textDetail }}</span>
            </template>
            <template v-else-if="col.slot === 'size'">
              <span class="cell-wrap">{{ formatBytes(row.size) }}</span>
            </template>
            <template v-else-if="col.slot === 'streams'">
              <span v-if="row.videoCount" class="stream-tag video">V{{ row.videoCount }}</span>
              <span v-if="row.audioCount" class="stream-tag audio">A{{ row.audioCount }}</span>
              <span v-if="row.textCount" class="stream-tag text">S{{ row.textCount }}</span>
            </template>
            <template v-else>
              <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has(col.prop) }">{{ (row as any)[col.prop] }}</span>
            </template>
          </template>
        </el-table-column>
      </el-table>
    </div>

    <!-- 表格视图：按归类分组 -->
    <div v-else-if="viewMode === 'table'" class="grouped-table-wrapper" :ref="(el: any) => observeTableContainer(el?.$el || el)">
      <div v-for="group in groupedTableData" :key="group.label" class="group-section">
        <div class="group-header">
          <span class="group-label">{{ group.label }}</span>
          <el-tag size="small" type="info">{{ group.count }} {{ t('个') }}</el-tag>
        </div>
        <el-table
          :data="group.rows"
          size="small"
          class="info-table"
          row-key="id"
          :max-height="Math.min(tableMaxHeight, 400)"
          border
          @sort-change="handleSortChange"
        >
          <!-- 序号列 -->
          <el-table-column width="60" align="center" resizable>
            <template #header><span>{{ t('序号') }}</span></template>
            <template #default="{ $index, row }">
              <span :class="{ 'idx-success': row.status === 'success', 'idx-error': row.status !== 'success' }">{{ $index + 1 }}</span>
            </template>
          </el-table-column>

          <!-- 文件名列 -->
          <el-table-column prop="name" :label="t('文件名')" :width="nameColWidth" sortable="custom" resizable>
            <template #header>
              <span class="col-header-text">{{ t('文件名') }}</span>
              <el-icon class="col-filter-btn" :class="{ 'filter-active': isFilterActive('name') }" @click.stop="openFilterPopover('name')"><Filter /></el-icon>
            </template>
            <template #default="{ row }">
              <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('name') }">{{ row.name }}</span>
            </template>
          </el-table-column>

          <!-- 动态数据列 -->
          <el-table-column
            v-for="col in visibleColumns"
            :key="col.prop"
            :prop="col.prop"
            :width="col.width"
            :align="col.align || 'center'"
            :sortable="col.sortable ? 'custom' : undefined"
            resizable
          >
            <template #header>
              <span class="col-header-text">{{ t(col.label) }}</span>
              <el-icon class="col-filter-btn" :class="{ 'filter-active': isFilterActive(col.prop) }" @click.stop="openFilterPopover(col.prop)"><Filter /></el-icon>
            </template>
            <template #default="{ row }">
              <template v-if="col.slot === 'health'">
                <el-popover placement="top" trigger="hover" width="280">
                  <template #reference>
                    <el-tag size="small" :type="getSeverityTagType(row.healthSeverity)">
                      {{ row.healthLabel }}
                    </el-tag>
                  </template>
                  <div class="health-popover">
                    <p v-if="!row.healthIssues.length">{{ t('未发现异常') }}</p>
                    <div v-for="issue in row.healthIssues" :key="`${issue.label}-${issue.detail}`" class="health-popover-item">
                      <el-tag size="small" :type="getSeverityTagType(issue.severity)">{{ t(issue.label) }}</el-tag>
                      <span>{{ issue.detail }}</span>
                    </div>
                  </div>
                </el-popover>
              </template>
              <template v-else-if="col.slot === 'duration'">
                <span class="cell-wrap">{{ row.duration }}</span>
              </template>
              <template v-else-if="col.slot === 'resolution'">
                <span class="cell-wrap">{{ row.resolution }}</span>
              </template>
              <template v-else-if="col.slot === 'bitrate'">
                <span class="cell-wrap">{{ row.bitrate }}</span>
              </template>
              <template v-else-if="col.slot === 'frameRate'">
                <span class="cell-wrap">{{ row.frameRate }}</span>
              </template>
              <template v-else-if="col.slot === 'hdr'">
                <el-tag v-if="row.hdrFormat && row.hdrFormat !== '-'" size="small" type="warning" class="cell-wrap">{{ row.hdrFormat }}</el-tag>
                <span v-else>-</span>
              </template>
              <template v-else-if="col.slot === 'audioCount'">
                <span v-if="row.audioCount" class="stream-tag audio">A{{ row.audioCount }}</span>
                <span v-else>-</span>
              </template>
              <template v-else-if="col.slot === 'textCount'">
                <span v-if="row.textCount" class="stream-tag text">S{{ row.textCount }}</span>
                <span v-else>-</span>
              </template>
              <template v-else-if="col.slot === 'audioSummary'">
                <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('audioSummary') }">{{ row.audioSummary }}</span>
              </template>
              <template v-else-if="col.slot === 'textSummary'">
                <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('textSummary') }">{{ row.textSummary }}</span>
              </template>
              <template v-else-if="col.slot === 'audioDetail'">
                <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('audioDetail') }">{{ row.audioDetail }}</span>
              </template>
              <template v-else-if="col.slot === 'textDetail'">
                <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has('textDetail') }">{{ row.textDetail }}</span>
              </template>
              <template v-else-if="col.slot === 'size'">
                <span class="cell-wrap">{{ formatBytes(row.size) }}</span>
              </template>
              <template v-else-if="col.slot === 'streams'">
                <span v-if="row.videoCount" class="stream-tag video">V{{ row.videoCount }}</span>
                <span v-if="row.audioCount" class="stream-tag audio">A{{ row.audioCount }}</span>
                <span v-if="row.textCount" class="stream-tag text">S{{ row.textCount }}</span>
              </template>
              <template v-else>
                <span class="cell-wrap" :class="{ 'dim-cell': highlightDiff && columnAnalysis.same.has(col.prop) }">{{ (row as any)[col.prop] }}</span>
              </template>
            </template>
          </el-table-column>
        </el-table>
      </div>
    </div>

    <!-- 入库进度 -->
    <div v-if="saving" class="progress-bar">
      <el-progress
        :percentage="Math.round((saveProgress.saved / saveProgress.total) * 100)"
        :indeterminate="saveProgress.phase === 'db'"
        :stroke-width="4"
      />
      <span class="summary-text">
        {{ saveProgress.phase === 'xml' ? t('正在获取 MediaInfo XML...') : saveProgress.phase === 'db' ? t('正在写入数据库...') : t('完成') }}
        ({{ saveProgress.saved }}/{{ saveProgress.total }})
      </span>
    </div>

    <!-- 空状态 -->
    <div v-if="!items.length && !importing" class="empty-state">
      <div class="empty-icon">🎬</div>
      <p class="empty-title">{{ t('导入视频文件以查看详细信息') }}</p>
      <p class="empty-desc">{{ t('支持文件夹导入或粘贴路径，可选择是否递归遍历子目录') }}</p>
    </div>

    <!-- 历史记录对话框 -->
    <el-dialog v-model="historyVisible" :title="t('历史记录')" width="clamp(600px, 85vw, 1400px)" top="5vh" append-to-body>
      <div style="margin-bottom: 10px; display: flex; align-items: center; gap: 12px; flex-wrap: wrap;">
        <el-popover v-model:visible="historyColPopoverVisible" placement="bottom-start" :width="480" trigger="click">
          <template #reference>
            <el-button size="small" text type="primary">{{ t('选择列') }} ({{ historyColKeys.length }}/{{ HISTORY_ALL_COLUMNS.length }})</el-button>
          </template>
          <div style="max-height: 50vh; overflow-y: auto;">
            <div v-for="(cols, group) in historyGroupedCols" :key="group" style="margin-bottom: 8px;">
              <div style="font-size: 12px; font-weight: 600; color: #606266; margin-bottom: 4px;">{{ GROUP_LABELS[group] }}</div>
              <el-checkbox-group v-model="historyColKeys" size="small">
                <el-checkbox v-for="col in cols" :key="col.key" :value="col.key" style="margin-right: 12px; margin-bottom: 2px;">
                  {{ col.label }}
                </el-checkbox>
              </el-checkbox-group>
            </div>
          </div>
          <div style="margin-top: 8px; display: flex; gap: 8px;">
            <el-button size="small" text type="primary" @click="historyColKeys = HISTORY_ALL_COLUMNS.map(c => c.key)">全选</el-button>
            <el-button size="small" text @click="historyColKeys = [...HISTORY_DEFAULT_KEYS]">恢复默认</el-button>
          </div>
        </el-popover>
        <span style="font-size: 12px; color: #909399;">{{ historyRecords.length }} / {{ historyCount }} {{ t('条记录') }}</span>
      </div>
      <el-table
        :data="historyRecords"
        size="small"
        border
        v-loading="historyLoading"
        @selection-change="(rows: VideoRecordRow[]) => historySelection = rows.map(r => r.id)"
        max-height="60vh"
      >
        <el-table-column type="selection" width="40" fixed />
        <el-table-column v-if="historyColKeys.includes('name')" prop="name" :label="t('文件名')" min-width="200" show-overflow-tooltip fixed />
        <el-table-column v-if="historyColKeys.includes('format')" prop="format" :label="t('格式')" width="80" align="center" />
        <el-table-column v-if="historyColKeys.includes('resolution')" :label="t('分辨率')" width="120" align="center">
          <template #default="{ row }">{{ row.width && row.height ? `${row.width}×${row.height}` : '-' }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('codec')" prop="codec" :label="t('编码')" width="100" align="center" />
        <el-table-column v-if="historyColKeys.includes('frame_rate')" prop="frame_rate" :label="t('帧率')" width="80" align="center" />
        <el-table-column v-if="historyColKeys.includes('duration')" :label="t('时长')" width="90" align="center">
          <template #default="{ row }">{{ row.duration_ms ? formatDurationMs(row.duration_ms) : '-' }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('overall_bit_rate')" prop="overall_bit_rate" :label="t('码率')" width="120" align="center" />
        <el-table-column v-if="historyColKeys.includes('size')" :label="t('文件大小')" width="100" align="center">
          <template #default="{ row }">{{ formatBytes(row.size) }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('scanned_at')" prop="scanned_at" :label="t('入库时间')" width="160" align="center" />
        <el-table-column v-if="historyColKeys.includes('bit_depth')" label="位深" width="80" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'bit_depth') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('hdr_format')" label="HDR" min-width="150" show-overflow-tooltip>
          <template #default="{ row }">{{ getExtraField(row, 'hdr_format') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('scan_type')" label="扫描方式" width="90" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'scan_type') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('color_space')" label="色彩空间" width="90" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'color_space') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('color_primaries')" label="色域" width="100" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'color_primaries') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('transfer_characteristics')" label="传输特性" width="110" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'transfer_characteristics') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('chroma_subsampling')" label="色度采样" width="90" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'chroma_subsampling') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('format_profile')" label="编码Profile" min-width="120" show-overflow-tooltip>
          <template #default="{ row }">{{ getExtraField(row, 'format_profile') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('video_bit_rate')" label="视频码率" width="110" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'video_bit_rate') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('video_stream_size')" label="视频流大小" width="110" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'video_stream_size') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('video_language')" label="视频语言" width="100" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'video_language') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('channels')" label="声道" width="100" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'channels') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('channel_layout')" label="声道布局" width="120" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'channel_layout') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('audio_codec')" label="音频编码" width="110" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'audio_codec') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('audio_bit_rate')" label="音频码率" width="110" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'audio_bit_rate') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('sample_rate')" label="采样率" width="90" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'sample_rate') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('audio_language')" label="音频语言" width="100" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'audio_language') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('audio_stream_size')" label="音频流大小" width="110" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'audio_stream_size') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('text_count')" label="字幕数" width="80" align="center">
          <template #default="{ row }">{{ getExtraField(row, 'text_count') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('text_languages')" label="字幕语言" min-width="150" show-overflow-tooltip>
          <template #default="{ row }">{{ getExtraField(row, 'text_languages') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('writing_application')" label="封装工具" min-width="150" show-overflow-tooltip>
          <template #default="{ row }">{{ getExtraField(row, 'writing_application') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('encoded_library')" label="编码库" min-width="150" show-overflow-tooltip>
          <template #default="{ row }">{{ getExtraField(row, 'encoded_library') }}</template>
        </el-table-column>
        <el-table-column v-if="historyColKeys.includes('path')" prop="path" label="文件路径" min-width="300" show-overflow-tooltip />
      </el-table>
      <div v-if="historyHasMore" style="text-align: center; padding: 10px 0;">
        <el-button size="small" :loading="historyLoading" @click="loadMoreHistory">
          {{ t('加载更多') }} ({{ historyRecords.length }}/{{ historyCount }})
        </el-button>
      </div>
      <div v-else-if="historyRecords.length" style="text-align: center; padding: 6px 0; font-size: 12px; color: #909399;">
        {{ t('已加载全部') }} {{ historyRecords.length }} {{ t('条记录') }}
      </div>
      <template #footer>
        <el-button type="danger" plain :disabled="!historySelection.length" @click="deleteSelectedHistory">
          {{ t('删除选中') }}{{ historySelection.length ? ` (${historySelection.length})` : '' }}
        </el-button>
        <el-button type="primary" @click="loadHistoryToView">
          {{ historySelection.length ? `${t('加载选中')} (${historySelection.length})` : t('加载全部') }}
        </el-button>
        <el-button @click="historyVisible = false">{{ t('关闭') }}</el-button>
      </template>
    </el-dialog>

    <!-- 输出目录模式选择对话框 -->
    <el-dialog v-model="outputModeDialogVisible" title="选择输出方式" width="clamp(300px, 40vw, 440px)" append-to-body>
      <div style="display: flex; flex-direction: column; gap: 16px;">
        <el-radio-group v-model="outputMode" style="display: flex; flex-direction: column; gap: 12px;">
          <el-radio value="same">
            <div>
              <div style="font-weight: 600;">源目录内生成</div>
              <div style="font-size: 12px; color: #8b8fa3; margin-top: 2px;">在每个视频所在目录下生成对应的 MediaInfo 文件</div>
            </div>
          </el-radio>
          <el-radio value="folder">
            <div>
              <div style="font-weight: 600;">集中导出到指定目录</div>
              <div style="font-size: 12px; color: #8b8fa3; margin-top: 2px;">所有导出文件统一保存到一个目录，默认目录名如 mediainfo_txt_20260513182112</div>
            </div>
          </el-radio>
        </el-radio-group>
      </div>
      <template #footer>
        <el-button @click="outputModeDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="confirmOutputMode">确定导出</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.video-info-view {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.table-wrapper {
  border: 1px solid rgba(20, 23, 31, 0.06);
  border-radius: 12px;
  overflow: hidden;
  background: #fff;
}
.info-table {
  width: 100%;
}
.info-table :deep(.el-table__body-wrapper) {
  overflow-x: auto;
}
.info-table :deep(.el-table__cell) {
  word-break: break-all;
  white-space: normal;
  line-height: 1.5;
}
.name-wrap {
  display: inline-block;
  word-break: break-all;
  white-space: normal;
  line-height: 1.5;
  max-width: 100%;
}

/* 所有单元格支持换行 */
.cell-wrap {
  display: inline-block;
  word-break: break-all;
  white-space: normal;
  line-height: 1.5;
  max-width: 100%;
}
/* el-tag 也支持换行（默认 inline-flex 不换行会导致截断） */
.info-table :deep(.el-tag) {
  display: inline-block;
  word-break: break-all;
  white-space: normal;
  max-width: 100%;
  height: auto;
  line-height: 1.5;
}

/* 列头筛选按钮 */
.col-header-text {
  margin-right: 2px;
}
.col-filter-btn {
  cursor: pointer;
  opacity: 0.3;
  font-size: 12px;
  vertical-align: middle;
  transition: opacity 0.15s, color 0.15s;
}
.col-filter-btn:hover {
  opacity: 0.8;
}
.col-filter-btn.filter-active {
  opacity: 1;
  color: #409eff;
}

/* 筛选弹窗 */
.filter-popover {
  position: fixed;
  top: 0; left: 0; right: 0; bottom: 0;
  z-index: 2000;
}
.filter-panel {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  background: #fff;
  border-radius: 10px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.18);
  padding: 16px;
  min-width: 240px;
  max-width: 320px;
  max-height: 400px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  z-index: 2001;
}
.filter-title {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-weight: 600;
  font-size: 14px;
  color: #303133;
}
.filter-search {
  margin: 0;
}
.filter-actions {
  display: flex;
  gap: 8px;
}
.filter-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-y: auto;
  max-height: 240px;
}
.filter-list :deep(.el-checkbox) {
  margin-right: 0;
  height: auto;
  padding: 3px 0;
}
.filter-list :deep(.el-checkbox__label) {
  word-break: break-all;
  white-space: normal;
  line-height: 1.4;
}

/* 序号列：成功=绿色，失败=红色 */
.idx-success {
  display: inline-block;
  width: 24px;
  height: 24px;
  line-height: 24px;
  text-align: center;
  border-radius: 6px;
  background: rgba(103, 194, 58, 0.15);
  color: #67c23a;
  font-weight: 600;
  font-size: 12px;
}
.idx-error {
  display: inline-block;
  width: 24px;
  height: 24px;
  line-height: 24px;
  text-align: center;
  border-radius: 6px;
  background: rgba(245, 108, 108, 0.15);
  color: #f56c6c;
  font-weight: 600;
  font-size: 12px;
}

/* 差异高亮：相同值的单元格大幅淡化+灰底 */
.dim-cell {
  opacity: 0.18;
  background: rgba(0, 0, 0, 0.05);
  border-radius: 3px;
  padding: 0 3px;
}

.stream-tag {
  display: inline-block;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  margin-right: 2px;
}
.stream-tag.video { background: rgba(64, 158, 255, 0.1); color: #409eff; }
.stream-tag.audio { background: rgba(103, 194, 58, 0.1); color: #67c23a; }
.stream-tag.text { background: rgba(230, 162, 60, 0.1); color: #e6a23c; }

.grouped-table-wrapper {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.group-section {
  border: 1px solid rgba(20, 23, 31, 0.06);
  border-radius: 12px;
  overflow: hidden;
  background: #fff;
}
.group-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: linear-gradient(135deg, #f0f4ff, #e8eeff);
  border-bottom: 1px solid rgba(20, 23, 31, 0.06);
}
.group-label {
  font-weight: 700;
  font-size: 14px;
  color: #2f73ff;
}

.info-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 14px;
  border-radius: 10px;
  border: 1px solid rgba(20, 23, 31, 0.06);
  background: linear-gradient(145deg, rgba(243, 246, 255, 0.96), rgba(227, 235, 255, 0.9));
  flex-wrap: wrap;
  gap: 8px;
}

.toolbar-left {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.toolbar-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  color: #1a1d26;
}
.toolbar-sub {
  font-size: 12px;
  color: #6d7387;
}

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.switch-field {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
}
.switch-label {
  font-size: 12px;
  color: #4b5570;
}

.progress-bar {
  margin: 4px 0;
}

.level-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 8px;
  border: 1px solid rgba(20, 23, 31, 0.06);
  background: #fff;
  flex-wrap: wrap;
}

.level-selector {
  display: flex;
  align-items: center;
  gap: 10px;
}
.level-label {
  font-size: 13px;
  font-weight: 600;
  color: #303133;
  white-space: nowrap;
}
.level-desc {
  font-size: 12px;
  color: #8b8fa3;
  flex: 1;
}
.level-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
}

.summary-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-radius: 8px;
  background: #f5f7fa;
}
.summary-text {
  font-size: 12px;
  color: #6d7387;
}

.card-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.health-card-wrapper {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.health-issues-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 8px 10px;
  border-radius: 8px;
  background: #fff8ed;
  border: 1px solid rgba(230, 162, 60, 0.18);
}
.health-popover {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.health-popover p {
  margin: 0;
  color: #67c23a;
}
.health-popover-item {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  line-height: 1.5;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px 20px;
  text-align: center;
}
.empty-icon {
  font-size: 36px;
  margin-bottom: 10px;
}
.empty-title {
  margin: 0 0 8px;
  font-size: 16px;
  font-weight: 600;
  color: #303133;
}
.empty-desc {
  margin: 0;
  font-size: 13px;
  color: #8b8fa3;
}
</style>
