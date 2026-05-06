<script setup lang="ts">
import type { ImageRow, VideoRow } from '@media-batch/types/media'
import { useSettings } from '@core/hooks/useSettings'
import { Delete } from '@element-plus/icons-vue'

type MediaKind = 'video' | 'image'

type TableVideo = VideoRow & { previewName: string; order: number }
type TableImage = ImageRow & { previewName: string; order: number }

const props = defineProps<{
  fileTypeTab: MediaKind
  tableVideos: TableVideo[]
  tableImages: TableImage[]
  visibleVideoColumns: { duration: boolean; resolution: boolean; bitrate: boolean; frameRate: boolean; size: boolean; preview: boolean }
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

const TABLE_MAX_ROWS = 48
const TABLE_ROW_HEIGHT = 42
const tableMaxHeight = TABLE_MAX_ROWS * TABLE_ROW_HEIGHT

const { t } = useSettings()
</script>

<template>
  <div class="panel-container">
    <div class="panel-header">
      <div>
        <p class="eyebrow">{{ t('信息提取') }}</p>
        <h3>{{ t('视频与图片列表') }}</h3>
      </div>
      <el-tabs
        :model-value="props.fileTypeTab"
        type="card"
        class="type-tabs"
        @update:modelValue="(val: any) => emit('update:fileTypeTab', val as MediaKind)"
      >
        <el-tab-pane :label="t('视频')" name="video" />
        <el-tab-pane :label="t('图片')" name="image" />
      </el-tabs>
    </div>

    <div v-if="props.fileTypeTab === 'video'" class="table-block">
      <div class="table-top">
        <div class="hint">{{ t('支持排序、全选、Hover 查看完整路径') }}</div>
        <div class="actions">
          <el-button size="small" text type="primary" @click="emit('clear', 'video')">{{ t('清空') }}</el-button>
        </div>
      </div>
      <el-table
        :data="props.tableVideos"
        size="small"
        class="table-shell"
        :max-height="tableMaxHeight"
        row-key="id"
        @sort-change="(payload: any) => emit('sortChange', 'video', { prop: payload?.prop || null, order: payload?.order || null })"
      >
        <el-table-column width="82" align="center">
          <template #header>
            <span>{{ t('序号') }}</span>
          </template>
          <template #default="{ row, $index }">
            <div class="index-with-delete">
              <el-button
                type="danger"
                link
                :icon="Delete"
                @click="emit('remove', 'video', row.id)"
              />
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
          :label="t('时长')"
          prop="durationSec"
          width="110"
          align="center"
          sortable
          :sort-method="(a: any, b: any) => (a.durationSec || 0) - (b.durationSec || 0)"
        >
          <template #default="{ row }">{{ props.formatDuration(row.durationSec) }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleVideoColumns.resolution"
          prop="resolution"
          :label="t('分辨率')"
          width="120"
          align="center"
          sortable
          :sort-method="(a: any, b: any) => (a.width || 0) * (a.height || 0) - (b.width || 0) * (b.height || 0)"
        >
          <template #default="{ row }">{{ (row.width ?? '-') }}×{{ (row.height ?? '-') }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleVideoColumns.bitrate"
          :label="t('码率')"
          width="110"
          align="center"
          sortable
          :sort-method="(a: any, b: any) => (a.bitrateMbps || 0) - (b.bitrateMbps || 0)"
        >
          <template #default="{ row }">
            <span v-if="row.bitrateMbps">{{ row.bitrateMbps.toFixed(2) }} Mbps</span>
            <span v-else>-</span>
          </template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleVideoColumns.frameRate"
          :label="t('帧率')"
          prop="frameRate"
          width="110"
          align="center"
        >
          <template #default="{ row }">{{ row.frameRate || '-' }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleVideoColumns.size"
          :label="t('文件大小')"
          width="130"
          align="center"
          sortable
          :sort-method="(a: any, b: any) => a.size - b.size"
        >
          <template #default="{ row }">{{ props.formatBytes(row.size) }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.showPreview && props.visibleVideoColumns.preview"
          :label="t('预览名称')"
          min-width="180"
          show-overflow-tooltip
        >
          <template #default="{ row }">{{ row.previewName }}</template>
        </el-table-column>
      </el-table>
    </div>

    <div v-if="props.fileTypeTab === 'image'" class="table-block">
      <div class="table-top">
        <div class="hint">{{ t('支持提示') }}</div>
        <div class="actions">
          <el-button size="small" text type="primary" @click="emit('clear', 'image')">{{ t('清空') }}</el-button>
        </div>
      </div>
      <el-table
        :data="props.tableImages"
        size="small"
        class="table-shell"
        :max-height="tableMaxHeight"
        row-key="id"
        @sort-change="(payload: any) => emit('sortChange', 'image', { prop: payload?.prop || null, order: payload?.order || null })"
      >
        <el-table-column width="82" align="center">
          <template #header>
            <span>{{ t('序号') }}</span>
          </template>
          <template #default="{ row, $index }">
            <div class="index-with-delete">
              <el-button
                type="danger"
                link
                :icon="Delete"
                @click="emit('remove', 'image', row.id)"
              />
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
          prop="resolution"
          :label="t('分辨率')"
          width="120"
          align="center"
          sortable
          :sort-method="(a: any, b: any) => (a.width || 0) * (a.height || 0) - (b.width || 0) * (b.height || 0)"
        >
          <template #default="{ row }">{{ row.width ? `${row.width}×${row.height ?? ''}` : '' }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.visibleImageColumns.device"
          prop="device"
          :label="t('拍摄设备')"
          min-width="140"
          show-overflow-tooltip
        />
        <el-table-column
          v-if="props.visibleImageColumns.takenAt"
          prop="takenAt"
          :label="t('拍摄时间')"
          width="160"
          sortable
        />
        <el-table-column
          v-if="props.visibleImageColumns.focalLength"
          prop="focalLength"
          :label="t('焦距')"
          width="110"
          align="center"
        />
        <el-table-column
          v-if="props.visibleImageColumns.size"
          :label="t('文件大小')"
          width="120"
          align="center"
          sortable
          :sort-method="(a: any, b: any) => a.size - b.size"
        >
          <template #default="{ row }">{{ props.formatBytes(row.size) }}</template>
        </el-table-column>
        <el-table-column
          v-if="props.showPreview && props.visibleImageColumns.preview"
          :label="t('预览名称')"
          min-width="160"
          show-overflow-tooltip
        >
          <template #default="{ row }">{{ row.previewName }}</template>
        </el-table-column>
      </el-table>
    </div>
  </div>
</template>

<style scoped>
.panel-container {
  height: 100%;
  display: flex;
  flex-direction: column;
}
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
  flex-shrink: 0;
}
.table-block {
  margin-top: 12px;
  border: 1px solid rgba(20, 23, 31, 0.04);
  border-radius: 12px;
  overflow: hidden;
  background: #fff;
  display: flex;
  flex-direction: column;
  flex: 1;
}
.table-top {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 12px;
  background: #f7f8fd;
  border-bottom: 1px solid rgba(20, 23, 31, 0.05);
  flex-shrink: 0;
}
.hint {
  color: #6d7387;
  font-size: 12px;
}
.actions {
  display: flex;
  gap: 8px;
}
.table-shell {
  flex: 1;
  min-height: 260px;
  height: 100%;
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
  color: #2f73ff;
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
