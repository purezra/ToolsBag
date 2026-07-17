import { computed, ref, type Ref } from 'vue'
import type { DisplayLevel, VideoInfoItem, HealthIssue, HealthSeverity } from '../types/media'
import { parseBitRateMbps, compareByOrder } from '../utils/media-utils'

// ==================== 列定义 ====================

export interface ColumnDef {
  prop: string
  label: string
  width: number
  minLevel: DisplayLevel
  maxLevel?: DisplayLevel
  align?: string
  sortable?: boolean
  slot?: string
  resizable?: boolean
}

interface ExtractedRow {
  id: number
  name: string
  path: string
  size: number
  status: string
  format: string
  duration: string
  durationMs: number
  resolution: string
  width: number
  height: number
  pixelCount: number
  resolutionTier: string
  orientation: string
  codec: string
  bitrate: string
  bitrateNum: number
  frameRate: string
  frameRateNum: number
  hdrFormat: string
  bitDepth: string
  chromaSubsampling: string
  colorSpace: string
  colorPrimaries: string
  transferCharacteristics: string
  formatProfile: string
  cabac: string
  refFrames: string
  encodedLibrary: string
  codecId: string
  videoCount: number
  audioCount: number
  textCount: number
  healthIssues: HealthIssue[]
  healthIssueCount: number
  healthSeverity: string
  healthLabel: string
  audioSummary: string
  textSummary: string
  audioDetail: string
  textDetail: string
}

type GroupByOption = 'none' | 'resolution' | 'resolutionTier' | 'orientation' | 'codec' | 'hdr' | 'bitDepth' | 'format' | 'frameRate'

interface TableGroup {
  label: string
  count: number
  rows: ExtractedRow[]
}

/** 健康工具函数接口，由上层传入 */
interface HealthFns {
  analyzeHealth: (item: VideoInfoItem) => HealthIssue[]
  getWorstSeverity: (issues: HealthIssue[]) => HealthSeverity | 'success'
  getSeverityTagType: (severity: HealthSeverity | 'success') => string
  buildAudioSummary: (item: VideoInfoItem) => string
  buildAudioDetail: (item: VideoInfoItem) => string
  buildTextSummary: (item: VideoInfoItem) => string
  buildTextDetail: (item: VideoInfoItem) => string
}

// ==================== 静态列定义 ====================

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
  { prop: 'hdrFormat', label: 'HDR', width: 100, minLevel: 'beginner', align: 'center', slot: 'hdr' },
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

// ==================== 工具函数 ====================

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

const LEVEL_ORDER: DisplayLevel[] = ['public', 'beginner', 'advanced', 'professional']

const LEVEL_DESC: Record<DisplayLevel, string> = {
  public: '基础文件信息、分辨率、帧率、流数量',
  beginner: '编码格式、码率、帧率、音轨/字幕数量',
  advanced: '位深、HDR、色度抽样、色域、音轨/字幕规格',
  professional: 'CABAC、编码库、编码档次、音轨/字幕详细参数',
}

// ==================== Composable ====================

export function useVideoInfoColumns(
  items: Ref<VideoInfoItem[]>,
  options: {
    level: Ref<DisplayLevel>
    searchQuery: Ref<string>
    emptyColMode: Ref<'right' | 'hide'>
    groupBy: Ref<GroupByOption>
    /** 健康工具函数 */
    health: HealthFns
  }
) {
  const { level, searchQuery, emptyColMode, groupBy, health } = options

  // === 等级显示控制 ===
  const levelIndex = computed(() => LEVEL_ORDER.indexOf(level.value))
  const show = (minLevel: DisplayLevel, maxLevel?: DisplayLevel) => {
    const idx = levelIndex.value
    if (idx < LEVEL_ORDER.indexOf(minLevel)) return false
    if (maxLevel && idx > LEVEL_ORDER.indexOf(maxLevel)) return false
    return true
  }

  // === 归类选项 ===
  const GROUP_OPTIONS = computed(() => [
    { label: '不归类', value: 'none' as GroupByOption },
    { label: '按分辨率', value: 'resolution' as GroupByOption },
    { label: '分辨率等级', value: 'resolutionTier' as GroupByOption },
    { label: '按方向', value: 'orientation' as GroupByOption },
    { label: '按编码', value: 'codec' as GroupByOption },
    { label: '按HDR', value: 'hdr' as GroupByOption },
    { label: '按位深', value: 'bitDepth' as GroupByOption },
    { label: '按封装格式', value: 'format' as GroupByOption },
    { label: '按帧率', value: 'frameRate' as GroupByOption },
  ])

  const LEVEL_OPTIONS = computed(() => [
    { label: '大众', value: 'public' as DisplayLevel },
    { label: '入门', value: 'beginner' as DisplayLevel },
    { label: '进阶', value: 'advanced' as DisplayLevel },
    { label: '专业', value: 'professional' as DisplayLevel },
  ])

  // === 搜索过滤 ===
  const filteredItems = computed(() => {
    if (!searchQuery.value.trim()) return items.value
    const q = searchQuery.value.toLowerCase()
    return items.value.filter(
      (item) =>
        item.name.toLowerCase().includes(q) ||
        item.path.toLowerCase().includes(q)
    )
  })

  // === 行数据提取 ===
  const extractRow = (item: VideoInfoItem): ExtractedRow => {
    const g = item.detail?.general
    const v = item.detail?.videoStreams?.[0]
    const w = v?.width || 0
    const h = v?.height || 0
    const healthIssues = health.analyzeHealth(item)
    const healthSeverity = health.getWorstSeverity(healthIssues)
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
      bitrateNum: parseBitRateMbps(v?.bitRate || g?.overallBitRate),
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
      audioSummary: health.buildAudioSummary(item),
      textSummary: health.buildTextSummary(item),
      audioDetail: health.buildAudioDetail(item),
      textDetail: health.buildTextDetail(item),
    }
  }

  // === 排序 ===
  const sortState = ref<{ prop: string | null; order: 'ascending' | 'descending' | null }>({ prop: null, order: null })

  const handleSortChange = (payload: { prop: string | null; order: 'ascending' | 'descending' | null }) => {
    sortState.value = payload
  }

  const sortRows = (rows: ExtractedRow[]) => {
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

  // === 列筛选 ===
  const columnFilters = ref<Map<string, Set<string>>>(new Map())
  const filterPopoverCol = ref<string | null>(null)
  const filterSearch = ref('')

  const allColumnUniqueValues = computed(() => {
    const rows = preFilterTableData.value
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

  const filterValueList = computed(() => {
    const col = filterPopoverCol.value
    if (!col) return []
    const all = allColumnUniqueValues.value.get(col) ?? []
    const q = filterSearch.value.toLowerCase().trim()
    if (!q) return all
    return all.filter(v => v.toLowerCase().includes(q))
  })

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
    columnFilters.value = new Map(filters)
  }

  const selectAllFilter = (prop: string) => {
    const filters = columnFilters.value
    filters.set(prop, new Set())
    columnFilters.value = new Map(filters)
  }

  const clearFilter = (prop: string) => {
    const filters = columnFilters.value
    filters.delete(prop)
    columnFilters.value = new Map(filters)
  }

  const isFilterActive = (prop: string): boolean => {
    const f = columnFilters.value.get(prop)
    return !!f && f.size > 0
  }

  const openFilterPopover = (prop: string) => {
    if (filterPopoverCol.value === prop) {
      filterPopoverCol.value = null
    } else {
      filterPopoverCol.value = prop
      filterSearch.value = ''
    }
  }

  const applyColumnFilters = (rows: ExtractedRow[]) => {
    const filters = columnFilters.value
    if (filters.size === 0) return rows
    return rows.filter(row => {
      for (const [prop, selected] of filters) {
        if (selected.size === 0) continue
        const val = String((row as any)[prop] ?? '')
        if (!selected.has(val)) return false
      }
      return true
    })
  }

  // === 数据管线 ===
  const preFilterTableData = computed(() => {
    const rows = filteredItems.value.map(item => extractRow(item))
    return sortRows(rows)
  })

  const flatTableData = computed(() => applyColumnFilters(preFilterTableData.value))

  const groupedTableData = computed<TableGroup[]>(() => {
    if (groupBy.value === 'none') return []
    const allRows = applyColumnFilters(preFilterTableData.value)
    const map = new Map<string, ExtractedRow[]>()

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

  // === 列分析 ===
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

  const visibleColumns = computed<ColumnDef[]>(() => {
    const { empty, same } = columnAnalysis.value
    const levelCols = ALL_COLUMNS.filter(c => show(c.minLevel, c.maxLevel))

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

    const result = [...normal, ...sameValue]
    if (emptyColMode.value === 'right') {
      result.push(...emptyValue)
    }
    if (level.value === 'beginner') {
      const hdr = levelCols.find(col => col.prop === 'hdrFormat')!
      const health = levelCols.find(col => col.prop === 'healthIssueCount')!
      const ordered = result.filter(col => col.prop !== hdr.prop && col.prop !== health.prop)
      const formatIndex = ordered.findIndex(col => col.prop === 'format')
      ordered.splice(formatIndex + 1, 0, hdr)
      ordered.push(health)
      return ordered
    }
    return result
  })

  // === 表格容器尺寸 ===
  const tableContainerWidth = ref(0)
  const tableContainerRef = ref<HTMLElement | null>(null)
  const windowHeight = ref(window.innerHeight)

  const tableMaxHeight = computed(() => {
    const overhead = 300
    return Math.max(400, windowHeight.value - overhead)
  })

  const FIXED_BASE_WIDTH = 60 + 100 + 100
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

  const destroyResizeObserver = () => {
    if (resizeObserver) {
      resizeObserver.disconnect()
      resizeObserver = null
    }
  }

  return {
    ALL_COLUMNS,
    GROUP_OPTIONS,
    LEVEL_OPTIONS,
    LEVEL_DESC,
    visibleColumns,
    columnAnalysis,
    columnFilters,
    filterPopoverCol,
    filterSearch,
    filterValueList,
    toggleFilterValue,
    selectAllFilter,
    clearFilter,
    isFilterActive,
    openFilterPopover,
    sortState,
    handleSortChange,
    filteredItems,
    preFilterTableData,
    flatTableData,
    groupedTableData,
    extractRow,
    tableContainerWidth,
    tableContainerRef,
    windowHeight,
    tableMaxHeight,
    nameColWidth,
    observeTableContainer,
    destroyResizeObserver,
  }
}
