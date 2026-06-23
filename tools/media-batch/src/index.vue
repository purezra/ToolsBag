<script setup lang="ts">
import { computed, nextTick, reactive, ref, defineAsyncComponent, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Plus, FolderAdd, Upload } from '@element-plus/icons-vue'
import { useFileSelect } from '@core/hooks/useFileSelect'
const MediaStatsPanel = defineAsyncComponent(() => import('./components/media-stats-panel.vue'))
import MediaTablePanel from './components/media-table-panel.vue'
import MediaTopBar from './components/media-top-bar.vue'
import MediaSettingsPanel from './components/media-settings-panel.vue'
import VideoInfoView from './components/video-info-view.vue'
import { useMediaBatch } from './hooks/useMediaBatch'
import { useSettings } from '@core/hooks/useSettings'
import type { VideoInfoItem, VideoRow } from './types/media'
import type { PickKind } from '@core/api/common'

// reactive 包装：让模板内 batch.xxx 自动解包 ref，避免在模板里到处 .value
const batch = reactive(useMediaBatch())
const { t } = useSettings()
const { pick } = useFileSelect()

const viewMode = ref<'batch' | 'exhibition'>('batch')
// 公共递归开关：两个视图共享
const recursive = ref(false)
// 公共导入状态
const importing = ref(false)

// 元数据视图首次访问后才挂载，避免初始页就预加载大组件
const exhibitionMounted = ref(false)
const videoInfoRef = ref<InstanceType<typeof VideoInfoView> | null>(null)

// 视图切换：batch→exhibition 时，若展览视图无数据但重命名视图有视频，
// 自动用重命名视图的路径发起详细提取（缓存命中则秒开）
watch(viewMode, async (v) => {
  if (v === 'exhibition') {
    exhibitionMounted.value = true
    await nextTick()
    const vi = videoInfoRef.value
    if (vi && !vi.hasData()) {
      const videoPaths = batch.videoRows.map(r => r.path).filter(Boolean)
      if (videoPaths.length) {
        vi.importByBatchPaths(videoPaths)
      }
    }
  }
})

// ==================== 公共导入逻辑 ====================
// 根据当前 viewMode 决定提取深度：
// - batch 模式：只提取重命名所需字段（轻量 probe）
// - exhibition 模式：提取完整元数据（详细 detailed），结果纳入缓存
const handleImport = async (kind: PickKind) => {
  if (importing.value) return
  const picked = await pick(kind)
  const paths = (picked || []).filter((p: string) => !/[\*\?\[\]]/.test(p))
  if (picked && paths.length !== picked.length) {
    ElMessage.warning(t('已忽略包含通配符的路径'))
  }
  if (!paths.length) return

  if (viewMode.value === 'exhibition') {
    // exhibition 模式：直接走详细提取
    await nextTick()
    const vi = videoInfoRef.value
    if (vi) {
      importing.value = true
      try {
        await vi.importByPaths(paths, recursive.value)
      } finally {
        importing.value = false
      }
    }
  } else {
    // batch 模式：走轻量提取，空文件夹兜底也在此处理
    // 同步递归值给 batch
    batch.recursive = recursive.value
    await batch.handleImportWithPaths(paths, kind)
    // 同步回可能被空文件夹逻辑修改的递归值
    recursive.value = batch.recursive
  }
}

const statsCardData = computed(() => ({
  basic: batch.basicStats,
  advanced: batch.advancedStats,
  buckets: batch.durationBuckets
}))

const tickerText = computed(() => {
  if (!batch.liveImports.length) return ''
  return batch.liveImports.map((item) => item.name).join(' · ')
})

// 视频元数据导出 → 视频图片重命名：直接注入扁平字段，无需二次后端扫描
// 异步化：先反馈 + 切到目标视图，让 mergeByPath 与表格重渲在下一帧再发生，避免按钮卡顿
const handleAddToRename = async (videoItems: VideoInfoItem[]) => {
  if (!videoItems.length) return
  const rows: VideoRow[] = videoItems.map((item) => ({
    id: 0, // mergeByPath 会重排
    name: item.name,
    path: item.path,
    size: item.size,
    mediaType: 'video' as const,
    status: 'success' as const,
    durationSec: item.durationSec,
    width: item.width,
    height: item.height,
    bitrateMbps: item.bitrateMbps,
  }))
  ElMessage.success(t('已加入重命名：{n} 个', { n: rows.length }))
  viewMode.value = 'batch'
  await nextTick()
  // 推到下一帧：让视图切换先完成，再做大数组合并/重排，避免主线程长任务
  requestAnimationFrame(() => {
    batch.videoRows = batch.mergeByPath(batch.videoRows, rows)
  })
}

// 视频图片重命名 → 视频元数据导出：切到导出视图并导入该视频的完整详情
const handleInspect = async (path: string) => {
  viewMode.value = 'exhibition'
  await nextTick()
  videoInfoRef.value?.inspectPath(path)
}
</script>

<template>
  <div class="media-tool tool-page">
    <!-- 公共工具栏：视图切换 + 导入按钮 + 递归开关 -->
    <div class="common-toolbar">
      <div class="toolbar-left">
        <el-segmented
          v-model="viewMode"
          :options="[
            { label: t('视频图片重命名'), value: 'batch' },
            { label: t('视频元数据导出'), value: 'exhibition' },
          ]"
          size="default"
        />
      </div>
      <div class="toolbar-right">
        <el-button :icon="Plus" size="small" :loading="importing" @click="handleImport('file')">
          {{ t('添加') }}
        </el-button>
        <el-button :icon="FolderAdd" size="small" :loading="importing" @click="handleImport('folder')">
          {{ t('文件夹') }}
        </el-button>
        <el-button :icon="Upload" size="small" plain :loading="importing" @click="handleImport('clipboard')">
          {{ t('粘贴') }}
        </el-button>
        <label class="switch-field">
          <span class="switch-label">{{ t('递归') }}</span>
          <el-switch v-model="recursive" size="small" />
        </label>
      </div>
    </div>

    <VideoInfoView
      v-if="exhibitionMounted"
      ref="videoInfoRef"
      v-show="viewMode === 'exhibition'"
      class="view-pane"
      :recursive="recursive"
      @add-to-rename="handleAddToRename"
    />

    <div v-show="viewMode === 'batch'" class="batch-view view-pane">
      <div v-if="batch.importProgress.active && tickerText" class="live-strip">
        <div class="live-strip__label">{{ t('实时导入') }}</div>
        <div class="live-strip__track">
          <div
            class="live-strip__text"
            :style="{ animationDuration: `${Math.max(12, tickerText.length / 6)}s` }"
          >
            {{ tickerText }}
          </div>
          <div
            class="live-strip__text"
            :style="{ animationDuration: `${Math.max(12, tickerText.length / 6)}s` }"
            aria-hidden="true"
          >
            {{ tickerText }}
          </div>
        </div>
        <div class="live-strip__count">{{ batch.liveImports.length }} {{ t('个') }}</div>
      </div>

      <div v-if="batch.importProgress.active" class="progress-bar flow-progress">
        <el-progress
          class="flow-progress__bar"
          :percentage="batch.importProgress.percent"
          :indeterminate="true"
          :stroke-width="6"
          :format="() => `${batch.importProgress.percent}% · ${batch.importProgress.batch}/${batch.importProgress.totalBatches}`"
        />
      </div>

      <div v-if="batch.importSummary" class="import-summary">
        <el-alert
          :title="`${t('导入完成')}：${batch.importSummary}`"
          :type="batch.failedItems.length ? 'warning' : 'success'"
          :closable="false"
          show-icon
        >
          <template #default>
            <el-button v-if="batch.failedItems.length" size="small" type="primary" plain @click="batch.showFailedDialog = true">
              {{ t('查看失败列表') }}（{{ batch.failedItems.length }}）
            </el-button>
          </template>
        </el-alert>
      </div>

      <el-row :gutter="12" class="panels">
        <el-col :span="16">
          <div class="panel-card full-height">
            <MediaTopBar
              :importing="batch.importing"
              :batch-size="batch.batchSize"
              :allow-auto-refresh="batch.allowAutoRefresh"
              @refresh="batch.refreshStats"
              @update:batchSize="(val) => (batch.batchSize = val)"
              @update:allowAutoRefresh="(val) => (batch.allowAutoRefresh = val)"
            />
            <MediaTablePanel
              v-model:file-type-tab="batch.fileTypeTab"
              :table-videos="batch.tableVideos"
              :table-images="batch.tableImages"
              :visible-video-columns="batch.visibleVideoColumns"
              :visible-image-columns="batch.visibleImageColumns"
              :show-preview="batch.showPreview"
              :format-bytes="batch.formatBytes"
              :format-duration="(val) => batch.formatDuration(val, batch.durationFormat)"
              @clear="batch.handleClear"
              @remove="batch.handleRemove"
              @copy="batch.copyName"
              @open="batch.openFolder"
              @sortChange="batch.handleTableSortChange"
              @inspect="handleInspect"
            />
          </div>
        </el-col>

        <el-col :span="8" class="right-col">
          <div class="panel-card right-card">
            <MediaSettingsPanel
              :file-type-tab="batch.fileTypeTab"
              :media-info-status="batch.mediaInfoStatus"
              :duration-format="batch.durationFormat"
              :rename-fields-video="batch.renameFieldsVideo"
              :rename-fields-image="batch.renameFieldsImage"
              :custom-text="batch.customText"
              :separator="batch.separator"
              :leading-zeros="batch.leadingZeros"
              :visible-video-columns="batch.visibleVideoColumns"
              :visible-image-columns="batch.visibleImageColumns"
              :show-preview="batch.showPreview"
              :rename-safety-summary="batch.renameSafetySummary"
              :can-undo-rename="batch.canUndoRename"
              :undo-stack-depth="batch.undoStackDepth"
              @update:durationFormat="(val) => (batch.durationFormat = val)"
              @update:customText="(val) => (batch.customText = val)"
              @update:separator="(val) => (batch.separator = val)"
              @update:leadingZeros="(val) => (batch.leadingZeros = val)"
              @toggleColumns="batch.toggleAllColumns"
              @toggleRenameField="batch.toggleRenameField"
              @moveField="batch.moveField"
              @previewRename="batch.previewRename"
              @applyRename="batch.applyRename"
              @undoRename="batch.undoLastRename"
              @applyPreset="batch.applyOrganizePreset"
              @toggleRenameFields="batch.toggleRenameFields"
            />
          </div>

          <div class="stats-wrapper">
            <MediaStatsPanel
              :file-type-tab="batch.fileTypeTab"
              :basic-stats="statsCardData.basic"
              :advanced-stats="statsCardData.advanced"
              :duration-buckets="statsCardData.buckets"
            />
          </div>
        </el-col>
      </el-row>

      <el-dialog v-model="batch.showFailedDialog" :title="t('导入失败列表')" width="520px">
        <el-alert
          v-if="batch.failedReasonStats.length"
          type="warning"
          :closable="false"
          show-icon
          class="failed-summary"
        >
          <template #title>{{ t('失败原因统计') }}</template>
          <div class="failed-reason-list">
            <div v-for="item in batch.failedReasonStats" :key="item.reason" class="failed-reason-row">
              <span>{{ item.reason }}</span>
              <el-tag size="small" type="warning">{{ item.count }}</el-tag>
            </div>
          </div>
        </el-alert>
        <el-table :data="batch.failedItems" size="small" row-key="path">
          <el-table-column prop="name" :label="t('文件名')" min-width="160" />
          <el-table-column prop="path" :label="t('路径')" min-width="200" show-overflow-tooltip />
          <el-table-column prop="reason" :label="t('原因')" min-width="120" show-overflow-tooltip />
        </el-table>
        <template #footer>
          <el-button type="primary" plain @click="batch.retryFailedImports">{{ t('重试失败项') }}</el-button>
          <el-button @click="batch.showFailedDialog = false">{{ t('关闭') }}</el-button>
        </template>
      </el-dialog>
    </div>
  </div>
</template>

<style scoped>
.media-tool {
  display: flex;
  flex-direction: column;
  gap: 6px;
  height: 100%;
  overflow: hidden;
  min-height: 0;
  background: var(--bg-page);
}
.common-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px;
  border-radius: var(--card-radius, 10px);
  border: 1px solid var(--card-border, rgba(20, 23, 31, 0.06));
  background: linear-gradient(145deg, rgba(243, 246, 255, 0.96), rgba(227, 235, 255, 0.9));
  flex-shrink: 0;
  gap: 12px;
  flex-wrap: wrap;
}
.toolbar-left {
  display: flex;
  align-items: center;
}
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 6px;
}
.switch-field {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
}
.switch-label {
  font-size: 12px;
  color: #4b5570;
}
.view-pane {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.batch-view {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.live-strip {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-secondary);
  background: var(--bg-secondary);
  box-shadow: none;
  overflow: hidden;
}
.live-strip__label {
  padding: 3px 6px;
  border-radius: var(--radius-sm);
  background: var(--accent-light);
  color: var(--accent);
  font-weight: 700;
  font-size: 11px;
  white-space: nowrap;
}
.live-strip__track {
  position: relative;
  flex: 1;
  overflow: hidden;
  height: 18px;
}
.live-strip__text {
  position: absolute;
  left: 0;
  top: 0;
  white-space: nowrap;
  font-size: 12px;
  animation-name: strip-marquee;
  animation-timing-function: linear;
  animation-iteration-count: infinite;
}
.live-strip__text:nth-child(2) {
  left: 100%;
}
.live-strip__count {
  font-weight: 700;
  color: var(--accent);
  font-size: 12px;
  white-space: nowrap;
}
@keyframes strip-marquee {
  0% { transform: translateX(0); }
  100% { transform: translateX(-100%); }
}
.panels {
  width: 100%;
  align-items: stretch;
  flex-wrap: nowrap;
  flex: 1;
  overflow: hidden;
}
:deep(.panels .el-col) {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
:deep(.panels .right-col) {
  overflow: hidden;
  gap: 6px;
}
.panel-card {
  display: flex;
  flex-direction: column;
  width: 100%;
  border-radius: var(--card-radius);
  border: 1px solid var(--card-border);
  box-shadow: var(--card-shadow);
  background: var(--color-card);
  overflow: hidden;
}
.panel-card.full-height {
  flex: 1;
  overflow: hidden;
  min-height: 0;
}
.panel-card.right-card {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}
.stats-wrapper {
  flex-shrink: 0;
}
.progress-bar {
  margin: 4px 0;
}
.import-summary {
  margin-bottom: 4px;
}
.flow-progress__bar :deep(.el-progress-bar__inner) {
  background: linear-gradient(90deg, #818CF8, #6366F1, #818CF8);
  background-size: 200% 100%;
  animation: flow 1.6s linear infinite;
}
@media (max-width: 1200px) {
  .panels {
    flex-wrap: wrap;
  }
}
@keyframes flow {
  0% { background-position: 0 0; }
  100% { background-position: -200% 0; }
}

.failed-summary {
  margin-bottom: 8px;
}
.failed-reason-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.failed-reason-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 6px;
}
</style>
