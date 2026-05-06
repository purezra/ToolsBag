<script setup lang="ts">
import { computed, onUnmounted, reactive, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { FolderAdd, Upload, Grid, List } from '@element-plus/icons-vue'
import { useFileSelect } from '@core/hooks/useFileSelect'
import { useSettings } from '@core/hooks/useSettings'
import { formatBytes } from '@core/utils/format'
import { importDetailedVideoInfo } from '../api/media-batch'
import VideoInfoDetailCard from './video-info-detail-card.vue'
import type { DisplayLevel, VideoInfoItem } from '../types/media'

const { pick } = useFileSelect()
const { t } = useSettings()

const level = ref<DisplayLevel>('beginner')
const recursive = ref(false)
const importing = ref(false)
const items = ref<VideoInfoItem[]>([])
const expandedIds = ref<Set<number>>(new Set())
const importSummary = ref('')
const searchQuery = ref('')
const viewMode = ref<'card' | 'table'>('card')

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
type GroupByOption = 'none' | 'resolution' | 'codec' | 'hdr' | 'bitDepth' | 'format' | 'frameRate'
const groupBy = ref<GroupByOption>('none')

const GROUP_OPTIONS = computed(() => [
  { label: t('不归类'), value: 'none' as GroupByOption },
  { label: t('按分辨率'), value: 'resolution' as GroupByOption },
  { label: t('按编码'), value: 'codec' as GroupByOption },
  { label: t('按HDR'), value: 'hdr' as GroupByOption },
  { label: t('按位深'), value: 'bitDepth' as GroupByOption },
  { label: t('按封装格式'), value: 'format' as GroupByOption },
  { label: t('按帧率'), value: 'frameRate' as GroupByOption },
])

// 从 VideoInfoItem 提取表格行
const extractRow = (item: VideoInfoItem) => {
  const g = item.detail?.general
  const v = item.detail?.videoStreams?.[0]
  return {
    id: item.id,
    name: item.name,
    path: item.path,
    size: item.size,
    status: item.status,
    format: g?.format || '-',
    duration: g?.duration || '-',
    durationMs: g?.durationMs || 0,
    resolution: v ? `${v.width}×${v.height}` : '-',
    width: v?.width || 0,
    height: v?.height || 0,
    pixelCount: v ? (v.width || 0) * (v.height || 0) : 0,
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
    // 专业级
    cabac: v?.cabac || '-',
    refFrames: v?.formatSettingsRefFrames || '-',
    encodedLibrary: v?.encodedLibrary || '-',
    codecId: v?.codecId || '-',
    videoCount: item.detail?.videoStreams?.length || 0,
    audioCount: item.detail?.audioStreams?.length || 0,
    textCount: item.detail?.textStreams?.length || 0,
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
    if (prop === 'size') return compareByOrder(a.size, b.size, order)
    if (prop === 'format') return order === 'ascending' ? a.format.localeCompare(b.format) : b.format.localeCompare(a.format)
    if (prop === 'codec') return order === 'ascending' ? a.codec.localeCompare(b.codec) : b.codec.localeCompare(a.codec)
    return 0
  })
}

// 无归类时的平铺数据
const flatTableData = computed(() => sortRows(filteredItems.value.map(extractRow)))

// 归类后的分组数据
interface TableGroup {
  label: string
  count: number
  rows: ReturnType<typeof extractRow>[]
}

const groupedTableData = computed<TableGroup[]>(() => {
  if (groupBy.value === 'none') return []
  const allRows = filteredItems.value.map(extractRow)
  const map = new Map<string, ReturnType<typeof extractRow>[]>()

  for (const row of allRows) {
    let key: string
    switch (groupBy.value) {
      case 'resolution': key = row.resolution || '未知'; break
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
  public: '基础文件信息、分辨率、帧率、声道',
  beginner: '编码格式、码率、位深、HDR、帧率模式',
  advanced: '编码档次、色度抽样、色域、压缩效率',
  professional: 'CABAC、编码器底层参数',
}

const LEVEL_ORDER: DisplayLevel[] = ['public', 'beginner', 'advanced', 'professional']
const levelIndex = computed(() => LEVEL_ORDER.indexOf(level.value))
const show = (minLevel: DisplayLevel) => levelIndex.value >= LEVEL_ORDER.indexOf(minLevel)

// 动态列宽：监听表格容器宽度，自动调节文件名列宽
const tableContainerWidth = ref(0)
const tableContainerRef = ref<HTMLElement | null>(null)

// 各等级固定列总宽度（不含文件名列）
// 序号(60) + 状态(70) + 公共列: 封装(100)+时长(90)+分辨率(120)+大小(100)+流(100) = 640
// 入门+300: 编码(100)+码率(120)+帧率(80)
// 进阶+570: 位深(70)+HDR(100)+色度(80)+编码档次(120)+色彩空间(100)+色域(100)
// 专业+430: CABAC(80)+参考帧(80)+编码库(140)+编码标识(130)
const FIXED_COL_WIDTHS: Record<DisplayLevel, number> = {
  public: 640,
  beginner: 940,
  advanced: 1510,
  professional: 1940,
}

const NAME_COL_MIN = 150
const NAME_COL_MAX = 800

const nameColWidth = computed(() => {
  const containerW = tableContainerWidth.value || 900
  const fixedW = FIXED_COL_WIDTHS[level.value]
  const available = containerW - fixedW - 20 // 20px buffer for borders/scrollbar
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
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
})

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

const handleImport = async (kind: 'folder' | 'clipboard') => {
  const picked = await pick(kind)
  const paths = (picked || []).filter((p: string) => !/[\*\?\[\]]/.test(p))
  if (!paths || paths.length === 0) return

  importing.value = true
  importProgress.active = true
  importProgress.percent = 0
  importSummary.value = ''
  expandedIds.value.clear()

  try {
    const resp = await importDetailedVideoInfo(paths, recursive.value)
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
    importProgress.percent = 100
  }
}

const handleClear = () => {
  items.value = []
  expandedIds.value.clear()
  importSummary.value = ''
  searchQuery.value = ''
}
</script>

<template>
  <div class="video-info-view">
    <!-- 顶部工具栏 -->
    <div class="info-toolbar">
      <div class="toolbar-left">
        <h3 class="toolbar-title">{{ t('视频信息展览') }}</h3>
        <span class="toolbar-sub">{{ t('导入视频文件，查看完整 MediaInfo 元数据') }}</span>
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
      </div>
    </div>

    <!-- 进度条 -->
    <div v-if="importProgress.active" class="progress-bar">
      <el-progress :percentage="importProgress.percent" :indeterminate="true" :stroke-width="4" />
    </div>

    <!-- 等级选择 + 统计 -->
    <div v-if="items.length" class="level-bar">
      <div class="level-selector">
        <span class="level-label">{{ t('显示等级') }}</span>
        <el-segmented v-model="level" :options="LEVEL_OPTIONS" size="small" />
      </div>
      <div class="level-desc">{{ t(LEVEL_DESC[level]) }}</div>
      <div class="level-actions">
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
        <el-button v-if="viewMode === 'card'" size="small" text type="primary" @click="expandAll">{{ t('全部展开') }}</el-button>
        <el-button v-if="viewMode === 'card'" size="small" text type="primary" @click="collapseAll">{{ t('全部收起') }}</el-button>
        <el-button size="small" text type="danger" @click="handleClear">{{ t('清空') }}</el-button>
      </div>
    </div>

    <!-- 导入统计 -->
    <div v-if="importSummary" class="summary-bar">
      <el-tag type="success" size="small">{{ t('成功') }} {{ successCount }}</el-tag>
      <el-tag v-if="failedCount" type="danger" size="small">{{ t('失败') }} {{ failedCount }}</el-tag>
      <span class="summary-text">{{ importSummary }}</span>
    </div>

    <!-- 卡片视图 -->
    <div v-if="viewMode === 'card'" class="card-list">
      <VideoInfoDetailCard
        v-for="item in filteredItems"
        :key="item.id"
        :item="item"
        :level="level"
        :expanded="expandedIds.has(item.id)"
        @toggle="toggleExpand(item.id)"
      />
    </div>

    <!-- 表格视图：无归类 -->
    <div v-else-if="groupBy === 'none'" class="table-wrapper" :ref="(el: any) => observeTableContainer(el?.$el || el)">
      <el-table
        :data="flatTableData"
        size="small"
        class="info-table"
        row-key="id"
        :max-height="520"
        border
        @sort-change="handleSortChange"
      >
        <el-table-column width="60" align="center" resizable>
          <template #header><span>{{ t('序号') }}</span></template>
          <template #default="{ $index }">{{ $index + 1 }}</template>
        </el-table-column>
        <el-table-column prop="name" :label="t('文件名')" :width="nameColWidth" sortable="custom" resizable>
          <template #default="{ row }">
            <span class="name-wrap">{{ row.name }}</span>
          </template>
        </el-table-column>
        <el-table-column v-if="show('public')" prop="format" :label="t('封装格式')" width="100" align="center" sortable="custom" resizable />
        <el-table-column v-if="show('public')" prop="durationMs" :label="t('时长')" width="90" align="center" sortable="custom" resizable>
          <template #default="{ row }">{{ row.duration }}</template>
        </el-table-column>
        <el-table-column v-if="show('public')" prop="pixelCount" :label="t('分辨率')" width="120" align="center" sortable="custom" resizable>
          <template #default="{ row }">{{ row.resolution }}</template>
        </el-table-column>
        <el-table-column v-if="show('beginner')" prop="codec" :label="t('视频编码')" width="100" align="center" sortable="custom" resizable />
        <el-table-column v-if="show('beginner')" prop="bitrateNum" :label="t('码率')" width="120" align="center" sortable="custom" resizable>
          <template #default="{ row }">{{ row.bitrate }}</template>
        </el-table-column>
        <el-table-column v-if="show('beginner')" prop="frameRateNum" :label="t('帧率')" width="80" align="center" sortable="custom" resizable>
          <template #default="{ row }">{{ row.frameRate }}</template>
        </el-table-column>
        <el-table-column v-if="show('advanced')" prop="bitDepth" :label="t('位深')" width="70" align="center" resizable />
        <el-table-column v-if="show('advanced')" prop="hdrFormat" :label="t('HDR')" width="100" align="center" resizable>
          <template #default="{ row }">
            <el-tag v-if="row.hdrFormat && row.hdrFormat !== '-'" size="small" type="warning">{{ row.hdrFormat }}</el-tag>
            <span v-else>-</span>
          </template>
        </el-table-column>
        <el-table-column v-if="show('advanced')" prop="chromaSubsampling" :label="t('色度')" width="80" align="center" resizable />
        <el-table-column v-if="show('advanced')" prop="formatProfile" :label="t('编码档次')" width="120" align="center" show-overflow-tooltip resizable />
        <el-table-column v-if="show('advanced')" prop="colorSpace" :label="t('色彩空间')" width="100" align="center" resizable />
        <el-table-column v-if="show('advanced')" prop="colorPrimaries" :label="t('色域')" width="100" align="center" show-overflow-tooltip resizable />
        <el-table-column v-if="show('professional')" prop="cabac" label="CABAC" width="80" align="center" resizable />
        <el-table-column v-if="show('professional')" prop="refFrames" :label="t('参考帧')" width="80" align="center" resizable />
        <el-table-column v-if="show('professional')" prop="encodedLibrary" :label="t('编码库')" width="140" align="center" show-overflow-tooltip resizable />
        <el-table-column v-if="show('professional')" prop="codecId" :label="t('编码标识')" width="130" align="center" show-overflow-tooltip resizable />
        <el-table-column v-if="show('public')" prop="size" :label="t('文件大小')" width="100" align="center" sortable="custom" resizable>
          <template #default="{ row }">{{ formatBytes(row.size) }}</template>
        </el-table-column>
        <el-table-column v-if="show('public')" :label="t('流')" width="100" align="center" resizable>
          <template #default="{ row }">
            <span v-if="row.videoCount" class="stream-tag video">V{{ row.videoCount }}</span>
            <span v-if="row.audioCount" class="stream-tag audio">A{{ row.audioCount }}</span>
            <span v-if="row.textCount" class="stream-tag text">S{{ row.textCount }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="status" :label="t('状态')" width="70" align="center" resizable>
          <template #default="{ row }">
            <el-tag :type="row.status === 'success' ? 'success' : 'danger'" size="small">
              {{ row.status === 'success' ? t('成功') : t('失败') }}
            </el-tag>
          </template>
        </el-table-column>
      </el-table>
    </div>

    <!-- 表格视图：按归类分组 -->
    <div v-else class="grouped-table-wrapper" :ref="(el: any) => observeTableContainer(el?.$el || el)">
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
          :max-height="400"
          border
          @sort-change="handleSortChange"
        >
          <el-table-column width="60" align="center" resizable>
            <template #header><span>{{ t('序号') }}</span></template>
            <template #default="{ $index }">{{ $index + 1 }}</template>
          </el-table-column>
          <el-table-column prop="name" :label="t('文件名')" :width="nameColWidth" sortable="custom" resizable>
            <template #default="{ row }">
              <span class="name-wrap">{{ row.name }}</span>
            </template>
          </el-table-column>
          <el-table-column v-if="show('public')" prop="format" :label="t('封装格式')" width="100" align="center" sortable="custom" resizable />
          <el-table-column v-if="show('public')" prop="durationMs" :label="t('时长')" width="90" align="center" sortable="custom" resizable>
            <template #default="{ row }">{{ row.duration }}</template>
          </el-table-column>
          <el-table-column v-if="show('public')" prop="pixelCount" :label="t('分辨率')" width="120" align="center" sortable="custom" resizable>
            <template #default="{ row }">{{ row.resolution }}</template>
          </el-table-column>
          <el-table-column v-if="show('beginner')" prop="codec" :label="t('视频编码')" width="100" align="center" sortable="custom" resizable />
          <el-table-column v-if="show('beginner')" prop="bitrateNum" :label="t('码率')" width="120" align="center" sortable="custom" resizable>
            <template #default="{ row }">{{ row.bitrate }}</template>
          </el-table-column>
          <el-table-column v-if="show('beginner')" prop="frameRateNum" :label="t('帧率')" width="80" align="center" sortable="custom" resizable>
            <template #default="{ row }">{{ row.frameRate }}</template>
          </el-table-column>
          <el-table-column v-if="show('advanced')" prop="bitDepth" :label="t('位深')" width="70" align="center" resizable />
          <el-table-column v-if="show('advanced')" prop="hdrFormat" :label="t('HDR')" width="100" align="center" resizable>
            <template #default="{ row }">
              <el-tag v-if="row.hdrFormat && row.hdrFormat !== '-'" size="small" type="warning">{{ row.hdrFormat }}</el-tag>
              <span v-else>-</span>
            </template>
          </el-table-column>
          <el-table-column v-if="show('advanced')" prop="chromaSubsampling" :label="t('色度')" width="80" align="center" resizable />
          <el-table-column v-if="show('advanced')" prop="formatProfile" :label="t('编码档次')" width="120" align="center" show-overflow-tooltip resizable />
          <el-table-column v-if="show('advanced')" prop="colorSpace" :label="t('色彩空间')" width="100" align="center" resizable />
          <el-table-column v-if="show('advanced')" prop="colorPrimaries" :label="t('色域')" width="100" align="center" show-overflow-tooltip resizable />
          <el-table-column v-if="show('professional')" prop="cabac" label="CABAC" width="80" align="center" resizable />
          <el-table-column v-if="show('professional')" prop="refFrames" :label="t('参考帧')" width="80" align="center" resizable />
          <el-table-column v-if="show('professional')" prop="encodedLibrary" :label="t('编码库')" width="140" align="center" show-overflow-tooltip resizable />
          <el-table-column v-if="show('professional')" prop="codecId" :label="t('编码标识')" width="130" align="center" show-overflow-tooltip resizable />
          <el-table-column v-if="show('public')" prop="size" :label="t('文件大小')" width="100" align="center" sortable="custom" resizable>
            <template #default="{ row }">{{ formatBytes(row.size) }}</template>
          </el-table-column>
          <el-table-column v-if="show('public')" :label="t('流')" width="100" align="center" resizable>
            <template #default="{ row }">
              <span v-if="row.videoCount" class="stream-tag video">V{{ row.videoCount }}</span>
              <span v-if="row.audioCount" class="stream-tag audio">A{{ row.audioCount }}</span>
              <span v-if="row.textCount" class="stream-tag text">S{{ row.textCount }}</span>
            </template>
          </el-table-column>
          <el-table-column prop="status" :label="t('状态')" width="70" align="center" resizable>
            <template #default="{ row }">
              <el-tag :type="row.status === 'success' ? 'success' : 'danger'" size="small">
                {{ row.status === 'success' ? t('成功') : t('失败') }}
              </el-tag>
            </template>
          </el-table-column>
        </el-table>
      </div>
    </div>

    <!-- 空状态 -->
    <div v-if="!items.length && !importing" class="empty-state">
      <div class="empty-icon">🎬</div>
      <p class="empty-title">{{ t('导入视频文件以查看详细信息') }}</p>
      <p class="empty-desc">{{ t('支持文件夹导入或粘贴路径，可选择是否递归遍历子目录') }}</p>
    </div>
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
.name-wrap {
  display: inline-block;
  word-break: break-all;
  white-space: normal;
  line-height: 1.5;
  max-width: 100%;
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
  gap: 10px;
  padding: 10px 16px;
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
  padding: 16px 20px;
  border-radius: 14px;
  border: 1px solid rgba(20, 23, 31, 0.06);
  background: linear-gradient(145deg, rgba(243, 246, 255, 0.96), rgba(227, 235, 255, 0.9));
  flex-wrap: wrap;
  gap: 12px;
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
  gap: 16px;
  padding: 12px 16px;
  border-radius: 12px;
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
  gap: 6px;
  flex-wrap: wrap;
}

.summary-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
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

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 80px 20px;
  text-align: center;
}
.empty-icon {
  font-size: 48px;
  margin-bottom: 16px;
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
