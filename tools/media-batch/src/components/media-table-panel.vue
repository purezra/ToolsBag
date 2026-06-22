<script setup lang="ts">
import type { ImageRow, VideoRow } from '@media-batch/types/media'
import { useSettings } from '@core/hooks/useSettings'
import { Delete, Upload } from '@element-plus/icons-vue'

type MediaKind = 'video' | 'image'

type TableVideo = VideoRow & { previewName: string; order: number }
type TableImage = ImageRow & { previewName: string; order: number }

const props = defineProps<{
  fileTypeTab: MediaKind
  tableVideos: TableVideo[]
  tableImages: TableImage[]
  visibleVideoColumns: { duration: boolean; resolution: boolean; bitrate: boolean; size: boolean; preview: boolean }
  visibleImageColumns: { resolution: boolean; device: boolean; takenAt: boolean; focalLength: boolean; size: boolean; preview: boolean }
  showPreview: boolean
  formatBytes: (value: number) => string
  formatDuration: (seconds: number | undefined) => string
}>()

const emit = defineEmits<{
  (e: 'clear', kind: MediaKind): void
  (e: 'remove', kind: MediaKind, id: number): void
  (e: 'copy', name: string): void
  (e: 'open', path: string): void
  (e: 'sortChange', kind: MediaKind, payload: { prop: string | null; order: 'ascending' | 'descending' | null }): void
  (e: 'update:fileTypeTab', value: MediaKind): void
}>()

const { t } = useSettings()

const hasData = (kind: MediaKind) =>
  kind === 'video' ? props.tableVideos.length > 0 : props.tableImages.length > 0
</script>

<template>
  <div class="table-wrap">
    <div class="table-header-bar">
      <div class="tab-switch">
        <el-segmented
          :model-value="props.fileTypeTab"
          :options="[
            { label: `${t('视频')} (${props.tableVideos.length})`, value: 'video' },
            { label: `${t('图片')} (${props.tableImages.length})`, value: 'image' },
          ]"
          size="small"
          @change="(val: any) => emit('update:fileTypeTab', val as MediaKind)"
        />
      </div>
      <div class="table-header-actions">
        <span class="hint">{{ t('文件名Hover查看路径') }}</span>
        <el-button size="small" text type="primary" @click="emit('clear', props.fileTypeTab)">{{ t('清空') }}</el-button>
      </div>
    </div>

    <!-- Video Table -->
    <div v-if="props.fileTypeTab === 'video'" class="table-block" :class="{ 'is-empty': !hasData('video') }">
      <el-table
        v-if="hasData('video')"
        :data="props.tableVideos"
        size="small"
        class="table-shell"
        height="100%"
        row-key="id"
        @sort-change="(payload: any) => emit('sortChange', 'video', { prop: payload?.prop || null, order: payload?.order || null })"
      >
        <el-table-column width="72" align="center">
          <template #header><span>{{ t('序号') }}</span></template>
          <template #default="{ row, $index }">
            <div class="index-with-delete">
              <el-button type="danger" link :icon="Delete" @click="emit('remove', 'video', row.id)" />
              <span>{{ $index + 1 }}</span>
            </div>
          </template>
        </el-table-column>
        <el-table-column prop="name" :label="t('文件名')" min-width="200">
          <template #default="{ row }">
            <el-popover placement="top" trigger="hover" width="240">
              <template #reference>
                <span class="link-like name-cell">{{ row.name }}</span>
              </template>
              <div class="name-popover">
                <p class="full-name" :title="row.name">{{ row.name }}</p>
                <div class="pop-actions">
                  <el-button text type="primary" size="small" @click="emit('copy', row.name)">{{ t('复制名称') }}</el-button>
                  <el-button text size="small" @click="emit('open', row.path)">{{ t('打开文件夹') }}</el-button>
                </div>
              </div>
            </el-popover>
          </template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleVideoColumns.duration"
          :label="t('时长')" prop="durationSec" width="100" align="center" sortable
          :sort-method="(a: any, b: any) => (a.durationSec || 0) - (b.durationSec || 0)"
        >
          <template #default="{ row }">{{ props.formatDuration(row.durationSec) }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleVideoColumns.resolution"
          prop="resolution" :label="t('分辨率')" width="110" align="center" sortable
          :sort-method="(a: any, b: any) => (a.width || 0) * (a.height || 0) - (b.width || 0) * (b.height || 0)"
        >
          <template #default="{ row }">{{ (row.width ?? '-') }}×{{ (row.height ?? '-') }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleVideoColumns.bitrate"
          :label="t('码率')" width="100" align="center" sortable
          :sort-method="(a: any, b: any) => (a.bitrateMbps || 0) - (b.bitrateMbps || 0)"
        >
          <template #default="{ row }">
            <span v-if="row.bitrateMbps">{{ row.bitrateMbps.toFixed(2) }} Mbps</span>
            <span v-else>-</span>
          </template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleVideoColumns.size"
          :label="t('文件大小')" width="110" align="center" sortable
          :sort-method="(a: any, b: any) => a.size - b.size"
        >
          <template #default="{ row }">{{ props.formatBytes(row.size) }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.showPreview && props.visibleVideoColumns.preview"
          :label="t('预览名称')" min-width="160" show-overflow-tooltip
        >
          <template #default="{ row }">{{ row.previewName }}</template>
        </el-table-column>
      </el-table>
      <div v-else class="dropzone">
        <el-icon :size="40" class="dropzone-icon"><Upload /></el-icon>
        <p class="dropzone-title">{{ t('暂无数据') }}</p>
        <p class="dropzone-hint">{{ t('点击上方「添加」或「文件夹」，或粘贴路径导入') }}</p>
      </div>
    </div>

    <!-- Image Table -->
    <div v-if="props.fileTypeTab === 'image'" class="table-block" :class="{ 'is-empty': !hasData('image') }">
      <el-table
        v-if="hasData('image')"
        :data="props.tableImages"
        size="small"
        class="table-shell"
        height="100%"
        row-key="id"
        @sort-change="(payload: any) => emit('sortChange', 'image', { prop: payload?.prop || null, order: payload?.order || null })"
      >
        <el-table-column width="72" align="center">
          <template #header><span>{{ t('序号') }}</span></template>
          <template #default="{ row, $index }">
            <div class="index-with-delete">
              <el-button type="danger" link :icon="Delete" @click="emit('remove', 'image', row.id)" />
              <span>{{ $index + 1 }}</span>
            </div>
          </template>
        </el-table-column>
        <el-table-column prop="name" :label="t('文件名')" min-width="180">
          <template #default="{ row }">
            <el-popover placement="top" trigger="hover" width="240">
              <template #reference>
                <span class="link-like name-cell">{{ row.name }}</span>
              </template>
              <div class="name-popover">
                <p class="full-name" :title="row.name">{{ row.name }}</p>
                <div class="pop-actions">
                  <el-button text type="primary" size="small" @click="emit('copy', row.name)">{{ t('复制名称') }}</el-button>
                  <el-button text size="small" @click="emit('open', row.path)">{{ t('打开文件夹') }}</el-button>
                </div>
              </div>
            </el-popover>
          </template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleImageColumns.resolution"
          prop="resolution" :label="t('分辨率')" width="110" align="center" sortable
          :sort-method="(a: any, b: any) => (a.width || 0) * (a.height || 0) - (b.width || 0) * (b.height || 0)"
        >
          <template #default="{ row }">{{ row.width ? `${row.width}×${row.height ?? ''}` : '' }}</template>
        </el-table-column>
        <el-table-column v-if="props.visibleImageColumns.device" prop="device" :label="t('拍摄设备')" min-width="130" show-overflow-tooltip />
        <el-table-column v-if="props.visibleImageColumns.takenAt" prop="takenAt" :label="t('拍摄时间')" width="150" sortable />
        <el-table-column v-if="props.visibleImageColumns.focalLength" prop="focalLength" :label="t('焦距')" width="90" align="center" />
        <el-table-column
          v-if="props.visibleImageColumns.size"
          :label="t('文件大小')" width="110" align="center" sortable
          :sort-method="(a: any, b: any) => a.size - b.size"
        >
          <template #default="{ row }">{{ props.formatBytes(row.size) }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.showPreview && props.visibleImageColumns.preview"
          :label="t('预览名称')" min-width="160" show-overflow-tooltip
        >
          <template #default="{ row }">{{ row.previewName }}</template>
        </el-table-column>
      </el-table>
      <div v-else class="dropzone">
        <el-icon :size="40" class="dropzone-icon"><Upload /></el-icon>
        <p class="dropzone-title">{{ t('暂无数据') }}</p>
        <p class="dropzone-hint">{{ t('点击上方「添加」或「文件夹」，或粘贴路径导入') }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.table-wrap {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: 0 10px 10px;
}
.table-header-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 0;
  flex-shrink: 0;
}
.tab-switch :deep(.el-segmented) {
  --el-segmented-item-selected-color: var(--text-inverse);
}
.table-header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
.hint {
  color: var(--text-muted);
  font-size: 11px;
}
.table-block {
  border: 1px solid var(--border-secondary);
  border-radius: var(--radius-md);
  overflow: hidden;
  background: var(--bg-primary);
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.table-block.is-empty {
  border-style: dashed;
  border-color: var(--border-primary);
}
.table-shell {
  flex: 1;
  min-height: 0;
  height: 100%;
  overflow-x: auto;
}
.dropzone {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 32px 16px;
  cursor: default;
}
.dropzone-icon {
  color: var(--text-muted);
  opacity: 0.5;
}
.dropzone-title {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-secondary);
}
.dropzone-hint {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
}
.name-popover {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.name-popover .full-name {
  margin: 0;
  font-weight: 700;
  word-break: break-all;
}
.name-popover .pop-actions {
  display: flex;
  gap: 8px;
}
.link-like {
  color: var(--accent);
  cursor: pointer;
}
.name-cell {
  display: inline-block;
  max-width: calc(85vw - 120px);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.index-with-delete {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
</style>
