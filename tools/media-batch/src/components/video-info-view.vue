<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, shallowRef, watch } from 'vue'
import { Grid, List, Filter, Download, Coin } from '@element-plus/icons-vue'
import { listen } from '@tauri-apps/api/event'
import { hasTauriRuntime } from '@core/utils/tauri'
import { formatBytes } from '@core/utils/format'
import { useSettings } from '@core/hooks/useSettings'
import VideoInfoDetailCard from './video-info-detail-card.vue'
import { useVideoInfoHealth } from '../composables/useVideoInfoHealth'
import { useVideoInfoColumns } from '../composables/useVideoInfoColumns'
import { useVideoInfoActions, type ExportFormat } from '../composables/useVideoInfoActions'
import { paginateRows } from '../utils/media-utils'
import type { DisplayLevel, VideoInfoItem } from '../types/media'
import type { VideoRecordRow } from '../api/video-db'

const props = defineProps<{
  recursive: boolean
}>()

const emit = defineEmits<{
  (e: 'addToRename', items: VideoInfoItem[]): void
}>()

const { t } = useSettings()

// ==================== 核心状态 ====================
const items = shallowRef<VideoInfoItem[]>([])
const level = ref<DisplayLevel>('beginner')
const expandedIds = ref<Set<number>>(new Set())
const importSummary = ref('')
const searchQuery = ref('')
const viewMode = ref<'card' | 'table'>('table')
const emptyColMode = ref<'right' | 'hide'>('right')
const highlightDiff = ref(false)
const groupBy = ref<'none' | 'resolution' | 'resolutionTier' | 'orientation' | 'codec' | 'hdr' | 'bitDepth' | 'format' | 'frameRate'>('none')
const RESULT_PAGE_SIZE = 50
const resultPage = ref(1)
const formatElapsed = (milliseconds: number) => milliseconds < 1000
  ? `${Math.round(milliseconds)} ms`
  : `${(milliseconds / 1000).toFixed(2)} s`

// ==================== Composable: 体检 ====================
const health = useVideoInfoHealth(items)
const {
  healthSummary,
  getHealthIssues,
  getSeverityTagType,
} = health

// ==================== Composable: 列系统 ====================
const columns = useVideoInfoColumns(items, {
  level,
  searchQuery,
  emptyColMode,
  groupBy,
  health: {
    analyzeHealth: health.analyzeHealth,
    getWorstSeverity: health.getWorstSeverity,
    getSeverityTagType: health.getSeverityTagType,
    buildAudioSummary: health.buildAudioSummary,
    buildAudioDetail: health.buildAudioDetail,
    buildTextSummary: health.buildTextSummary,
    buildTextDetail: health.buildTextDetail,
  },
})
const {
  LEVEL_OPTIONS, LEVEL_DESC, GROUP_OPTIONS,
  visibleColumns, columnAnalysis, columnFilters, filterPopoverCol, filterSearch, filterValueList,
  toggleFilterValue, selectAllFilter, clearFilter, isFilterActive, openFilterPopover,
  handleSortChange, flatTableData, groupedTableData, filteredItems,
  tableMaxHeight, nameColWidth, observeTableContainer, destroyResizeObserver,
  windowHeight,
} = columns

// ==================== Composable: 操作 ====================
// ponytail: 递归值来自父组件 props，用 computed 包装为 Ref 以满足 composable 签名
const recursiveRef = computed(() => props.recursive)
const actions = useVideoInfoActions(items, {
  importSummary,
  expandedIds,
  searchQuery,
  level,
  recursive: recursiveRef as any,
  emit,
  preFilterTableData: columns.preFilterTableData,
  flatTableData: columns.flatTableData,
  visibleColumns: columns.visibleColumns,
  healthSummary: health.healthSummary,
  t,
})
const {
  importing, importProgress,
  lastImportTiming,
  saving, saveProgress, handleSaveToDb,
  handleClear, handleAddToRename,
  historyCount, openHistory, historyVisible,
  historyRecords, historyLoading, historySelection, historyHasMore,
  historyColKeys, historyColPopoverVisible, HISTORY_ALL_COLUMNS, HISTORY_DEFAULT_KEYS,
  GROUP_LABELS, historyGroupedCols, getExtraField, formatDurationMs,
  loadMoreHistory, loadHistoryToView, deleteSelectedHistory,
  outputModeDialogVisible, outputMode, confirmOutputMode, exportData,
  successCount, failedCount, refreshHistoryCount,
} = actions

const flatPagination = computed(() => paginateRows(flatTableData.value, resultPage.value, RESULT_PAGE_SIZE))
const cardPagination = computed(() => paginateRows(filteredItems.value, resultPage.value, RESULT_PAGE_SIZE))
const resultTotal = computed(() => viewMode.value === 'card' ? filteredItems.value.length : flatTableData.value.length)
const pagedGroupedTableData = computed(() => {
  let remainingStart = (flatPagination.value.page - 1) * RESULT_PAGE_SIZE
  let remainingCount = RESULT_PAGE_SIZE
  const pageGroups = []

  for (const group of groupedTableData.value) {
    if (remainingCount <= 0) break
    if (remainingStart >= group.rows.length) {
      remainingStart -= group.rows.length
      continue
    }
    const rows = group.rows.slice(remainingStart, remainingStart + remainingCount)
    pageGroups.push({ ...group, rows })
    remainingCount -= rows.length
    remainingStart = 0
  }
  return pageGroups
})

watch(flatTableData, () => { resultPage.value = 1 })
watch([viewMode, groupBy], () => { resultPage.value = 1 })

// ==================== 展开/收起 ====================
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

// ==================== 进度监听 ====================
let unlistenProgress: (() => void) | null = null
let isMounted = false
let windowResizeHandler: (() => void) | null = null

onMounted(() => {
  isMounted = true
  refreshHistoryCount()

  if (!hasTauriRuntime()) return

  listen<{ stage: string; current: number; total: number; message: string }>('progress-update', (event) => {
    const payload = event.payload
    if (!payload || payload.stage !== 'video_info_import') return
    const total = Math.max(1, Number(payload.total || 1))
    const current = Math.min(total, Number(payload.current || 0))
    const message = String(payload.message || '')
    if (message === 'start') {
      importProgress.active = true
      importProgress.percent = 0
    } else if (message === 'done') {
      importProgress.percent = 100
      setTimeout(() => { importProgress.active = false }, 500)
    } else {
      importProgress.percent = Math.max(0, Math.min(100, Math.round((current / total) * 100)))
    }
  }).then(unlisten => {
    if (isMounted) {
      // 防御：理论上 onMounted 只触发一次，但保险起见清理旧的
      if (unlistenProgress) unlistenProgress()
      unlistenProgress = unlisten
    } else {
      unlisten()
    }
  })
})

onUnmounted(() => {
  isMounted = false
  destroyResizeObserver()
  if (windowResizeHandler) {
    window.removeEventListener('resize', windowResizeHandler)
    windowResizeHandler = null
  }
  if (unlistenProgress) {
    unlistenProgress()
    unlistenProgress = null
  }
})

// 窗口尺寸变化
windowResizeHandler = () => { windowHeight.value = window.innerHeight }
window.addEventListener('resize', windowResizeHandler)

defineExpose({
  inspectPath: (path: string) => actions.importByPaths([path]),
  /** 从重命名视图的视频列表导入详细元数据，已缓存则跳过 */
  importByBatchPaths: (paths: string[]) => actions.importByPaths(paths),
  /** 公共工具栏调用：给定路径 + 是否递归，走详细提取 */
  importByPaths: (paths: string[], isRecursive: boolean) => actions.importByPaths(paths, isRecursive),
  /** 当前是否已有数据（用于判断切换视图时是否需要自动导入） */
  hasData: () => items.value.length > 0,
})
</script>

<template>
  <div class="video-info-view">
    <!-- 顶部工具栏 -->
    <div class="info-toolbar">
      <div class="toolbar-left">
        <h3 class="toolbar-title">{{ t('视频元数据导出') }}</h3>
        <span class="toolbar-sub">{{ t('导入视频文件，检测 MediaInfo 元数据与潜在质量异常') }}</span>
      </div>
      <div class="toolbar-right">
        <el-button v-if="items.length" type="primary" plain round @click="handleAddToRename">
          {{ t('加入重命名') }}
        </el-button>
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
      <span v-if="lastImportTiming" class="summary-timing">
        {{ lastImportTiming.cached ? t('会话缓存') : t('元数据') }} {{ formatElapsed(lastImportTiming.metadataMs) }}
        · {{ t('界面显示') }} {{ formatElapsed(lastImportTiming.displayMs) }}
        · {{ t('总计') }} {{ formatElapsed(lastImportTiming.totalMs) }}
      </span>
    </div>

    <!-- 卡片视图 -->
    <div v-if="viewMode === 'card' && items.length" class="card-list">
      <div v-for="item in cardPagination.rows" :key="item.id" class="health-card-wrapper">
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
        :data="flatPagination.rows"
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
            <span :class="{ 'idx-success': row.status === 'success', 'idx-error': row.status !== 'success' }">{{ (flatPagination.page - 1) * RESULT_PAGE_SIZE + $index + 1 }}</span>
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
      <div v-for="group in pagedGroupedTableData" :key="group.label" class="group-section">
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

    <div v-if="resultTotal > RESULT_PAGE_SIZE" class="result-pagination">
      <el-pagination
        :current-page="flatPagination.page"
        :page-size="RESULT_PAGE_SIZE"
        :total="resultTotal"
        layout="total, prev, pager, next"
        small
        background
        @update:current-page="(page: number) => (resultPage = page)"
      />
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
  height: 100%;
  min-height: 0;
  overflow: hidden;
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
.result-pagination {
  display: flex;
  justify-content: flex-end;
  padding: 2px 4px;
  flex-shrink: 0;
}
.summary-timing {
  margin-left: auto;
  color: var(--text-muted);
  font-size: 11px;
  white-space: nowrap;
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

/* 修复：plain primary 按钮在浅色主题下 accent-light 底 + accent 字对比度过低，
   导致文字看似只有 hover 时出现。这里加深 plain 按钮的描边与字色饱和度，让文字稳定可见。 */
.toolbar-right :deep(.el-button.is-plain),
.level-actions :deep(.el-button.is-plain) {
  background: var(--bg-primary, #fff);
  border-color: var(--accent);
  color: var(--accent);
  font-weight: 600;
}
.toolbar-right :deep(.el-button.is-plain:hover),
.level-actions :deep(.el-button.is-plain:hover) {
  background: var(--accent);
  color: var(--text-inverse, #fff);
  border-color: var(--accent);
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
