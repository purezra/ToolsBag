<script setup lang="ts">
import { computed, reactive, ref, defineAsyncComponent } from 'vue'
import { CircleCheckFilled, WarningFilled } from '@element-plus/icons-vue'
const MediaStatsPanel = defineAsyncComponent(() => import('./components/media-stats-panel.vue'))
import MediaTablePanel from './components/media-table-panel.vue'
import MediaTopBar from './components/media-top-bar.vue'
import MediaSettingsPanel from './components/media-settings-panel.vue'
import VideoInfoView from './components/video-info-view.vue'
import { useMediaBatch } from './hooks/useMediaBatch'
import { useSettings } from '@core/hooks/useSettings'

const batch = reactive(useMediaBatch())
const { t } = useSettings()

const viewMode = ref<'batch' | 'exhibition'>('batch')

const statsCardData = computed(() => ({
  basic: batch.basicStats,
  advanced: batch.advancedStats,
  buckets: batch.durationBuckets
}))

const tickerText = computed(() => {
  if (!batch.liveImports.length) return ''
  return batch.liveImports.map((item) => item.name).join(' · ')
})
</script>

<template>
  <div class="media-tool">
    <!-- 视图模式切换 -->
    <div class="view-mode-bar">
      <el-segmented
        v-model="viewMode"
        :options="[
          { label: t('媒体整理'), value: 'batch' },
          { label: t('视频体检'), value: 'exhibition' },
        ]"
        size="default"
      />
    </div>

    <!-- 视频体检模式（KeepAlive 保留状态） -->
    <KeepAlive>
      <VideoInfoView v-if="viewMode === 'exhibition'" />
    </KeepAlive>

    <!-- 媒体整理模式 -->
    <template v-if="viewMode === 'batch'">
    <MediaTopBar
      :importing="batch.importing"
      :batch-size="batch.batchSize"
      :allow-auto-refresh="batch.allowAutoRefresh"
      :recursive="batch.recursive"
      @import="batch.handleImport"
      @refresh="batch.refreshStats"
      @update:batchSize="(val) => (batch.batchSize = val)"
      @update:allowAutoRefresh="(val) => (batch.allowAutoRefresh = val)"
      @update:recursive="(val) => (batch.recursive = val)"
    />

    <div v-if="batch.mediaInfoStatus.available" class="mediainfo-status">
      <el-tag type="success" size="small">
        <el-icon style="margin-right: 4px"><CircleCheckFilled /></el-icon>
        {{ t('已检测到 MediaInfo') }}，{{ t('路径') }}：{{ batch.mediaInfoStatus.path }}
      </el-tag>
    </div>
    <div v-else class="mediainfo-status">
      <el-tag type="warning" size="small">
        <el-icon style="margin-right: 4px"><WarningFilled /></el-icon>
        {{ t('未检测到 MediaInfo，将使用 ffprobe 作为备选') }}
      </el-tag>
    </div>

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
        <el-card class="panel-card full-height" shadow="never">
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
          />
        </el-card>
      </el-col>

      <el-col :span="8">
        <el-card class="panel-card" shadow="never">
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
            @update:durationFormat="(val) => (batch.durationFormat = val)"
            @update:customText="(val) => (batch.customText = val)"
            @update:separator="(val) => (batch.separator = val)"
            @update:leadingZeros="(val) => (batch.leadingZeros = val)"
            @toggleColumns="batch.toggleAllColumns"
            @moveField="batch.moveField"
            @previewRename="batch.previewRename"
            @applyRename="batch.applyRename"
            @undoRename="batch.undoLastRename"
            @applyPreset="batch.applyOrganizePreset"
            @toggleRenameFields="batch.toggleRenameFields"
          />
        </el-card>

        <MediaStatsPanel
          class="panel-card stats-panel-transparent"
          :file-type-tab="batch.fileTypeTab"
          :basic-stats="statsCardData.basic"
          :advanced-stats="statsCardData.advanced"
          :duration-buckets="statsCardData.buckets"
        />
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
    </template>
  </div>
</template>

<style scoped>
.media-tool {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.view-mode-bar {
  display: flex;
  justify-content: center;
  padding: 4px 0;
}
.live-strip {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border-radius: 14px;
  border: 1px solid rgba(20, 23, 31, 0.06);
  background: linear-gradient(135deg, #f8fbff, #eef2ff);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.6);
  overflow: hidden;
}
.live-strip__label {
  padding: 6px 10px;
  border-radius: 10px;
  background: rgba(47, 115, 255, 0.1);
  color: #2f73ff;
  font-weight: 700;
  white-space: nowrap;
}
.live-strip__track {
  position: relative;
  flex: 1;
  overflow: hidden;
  height: 20px;
}
.live-strip__text {
  position: absolute;
  left: 0;
  top: 0;
  white-space: nowrap;
  animation-name: strip-marquee;
  animation-timing-function: linear;
  animation-iteration-count: infinite;
}
.live-strip__text:nth-child(2) {
  left: 100%;
}
.live-strip__count {
  font-weight: 700;
  color: #2f73ff;
  white-space: nowrap;
}
@keyframes strip-marquee {
  0% {
    transform: translateX(0);
  }
  100% {
    transform: translateX(-100%);
  }
}
.panels {
  width: 100%;
  align-items: stretch;
  flex-wrap: nowrap;
}
:deep(.panels .el-col) {
  display: flex;
  flex-direction: column;
}
.panel-card {
  border-radius: 16px;
  border: 1px solid rgba(20, 23, 31, 0.06);
  display: flex;
  flex-direction: column;
  width: 100%;
}
.panel-card.full-height {
  height: 100%;
  min-height: 640px;
}
.panel-card.stats-panel-transparent {
  background: transparent;
  border: none;
  box-shadow: none;
}
.progress-bar {
  margin: 6px 0;
}
.import-summary {
  margin-bottom: 8px;
}
.flow-progress__bar :deep(.el-progress-bar__inner) {
  background: linear-gradient(90deg, #6f8cff, #9cb8ff, #6f8cff);
  background-size: 200% 100%;
  animation: flow 1.6s linear infinite;
}
@media (max-width: 1200px) {
  .panels {
    flex-wrap: wrap;
  }
}
@keyframes flow {
  0% {
    background-position: 0 0;
  }
  100% {
    background-position: -200% 0;
  }
}
.mediainfo-status {
  margin: 4px 0;
}

.failed-summary {
  margin-bottom: 10px;
}

.failed-reason-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.failed-reason-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
}
</style>
