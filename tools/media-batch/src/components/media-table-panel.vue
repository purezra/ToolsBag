<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { ImageRow, VideoRow } from '@media-batch/types/media'
import { paginateRows, type VideoFilterState } from '../utils/media-utils'
import { useSettings } from '@core/hooks/useSettings'
import { Delete, Upload } from '@element-plus/icons-vue'

type MediaKind = 'video' | 'image'

type TableVideo = VideoRow & { previewName: string; order: number }
type TableImage = ImageRow & { previewName: string; order: number }

const props = defineProps<{
  fileTypeTab: MediaKind
  tableVideos: TableVideo[]
  tableImages: TableImage[]
  videoTotalCount: number
  availableVideoFormats: string[]
  videoFilters: VideoFilterState
  selectedVideoPaths: string[]
  videoFilterMissingCount: number
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
  (e: 'inspect', path: string): void
  (e: 'toggleVideoSelection', path: string, selected: boolean): void
  (e: 'toggleAllVideoSelection', selected: boolean): void
  (e: 'updateVideoFilter', field: keyof VideoFilterState, value: any): void
  (e: 'resetVideoFilters'): void
  (e: 'sortChange', kind: MediaKind, payload: { prop: string | null; order: 'ascending' | 'descending' | null }): void
  (e: 'update:fileTypeTab', value: MediaKind): void
}>()

const { t } = useSettings()
const TABLE_PAGE_SIZE = 50
const videoPage = ref(1)
const imagePage = ref(1)

const hasData = (kind: MediaKind) =>
  kind === 'video' ? props.tableVideos.length > 0 : props.tableImages.length > 0

const selectedVideoPathSet = computed(() => new Set(props.selectedVideoPaths))
const allVisibleSelected = computed(() =>
  props.tableVideos.length > 0 && props.tableVideos.every(({ path }) => selectedVideoPathSet.value.has(path))
)
const someVisibleSelected = computed(() =>
  !allVisibleSelected.value && props.tableVideos.some(({ path }) => selectedVideoPathSet.value.has(path))
)
const videoPagination = computed(() => paginateRows(props.tableVideos, videoPage.value, TABLE_PAGE_SIZE))
const imagePagination = computed(() => paginateRows(props.tableImages, imagePage.value, TABLE_PAGE_SIZE))

watch(() => props.videoFilters, () => { videoPage.value = 1 }, { deep: true })
watch(() => props.tableVideos.length, () => { videoPage.value = videoPagination.value.page })
watch(() => props.tableImages.length, () => { imagePage.value = imagePagination.value.page })

const extensionOf = (name: string) => {
  const dot = name.lastIndexOf('.')
  return dot >= 0 ? name.slice(dot + 1).toUpperCase() : '-'
}
</script>

<template>
  <div class="table-wrap">
    <div v-if="props.fileTypeTab === 'video'" class="video-filter-bar">
      <div class="filter-control format-control">
        <span class="filter-label">{{ t('格式') }}</span>
        <el-select
          :model-value="props.videoFilters.formats"
          multiple
          collapse-tags
          collapse-tags-tooltip
          clearable
          size="small"
          :placeholder="t('全部格式')"
          @update:model-value="(value: string[]) => emit('updateVideoFilter', 'formats', value)"
        >
          <el-option
            v-for="format in props.availableVideoFormats"
            :key="format"
            :label="format.toUpperCase()"
            :value="format"
          />
        </el-select>
      </div>

      <div class="filter-control duration-control">
        <span class="filter-label">{{ t('时长') }}</span>
        <el-select
          :model-value="props.videoFilters.durationMode"
          size="small"
          @update:model-value="(value: string) => emit('updateVideoFilter', 'durationMode', value)"
        >
          <el-option :label="t('不限')" value="all" />
          <el-option :label="t('不超过')" value="at-most" />
          <el-option :label="t('不少于')" value="at-least" />
          <el-option :label="t('区间')" value="between" />
        </el-select>
        <el-input-number
          v-if="props.videoFilters.durationMode === 'at-least' || props.videoFilters.durationMode === 'between'"
          :model-value="props.videoFilters.durationMin"
          :min="0"
          :precision="1"
          :step="0.1"
          size="small"
          controls-position="right"
          @update:model-value="(value: number | undefined) => emit('updateVideoFilter', 'durationMin', value ?? 0)"
        />
        <span v-if="props.videoFilters.durationMode === 'between'" class="range-separator">–</span>
        <el-input-number
          v-if="props.videoFilters.durationMode === 'at-most' || props.videoFilters.durationMode === 'between'"
          :model-value="props.videoFilters.durationMax"
          :min="0"
          :precision="1"
          :step="0.1"
          size="small"
          controls-position="right"
          @update:model-value="(value: number | undefined) => emit('updateVideoFilter', 'durationMax', value ?? 0)"
        />
        <el-select
          v-if="props.videoFilters.durationMode !== 'all'"
          :model-value="props.videoFilters.durationUnit"
          class="unit-select"
          size="small"
          @update:model-value="(value: string) => emit('updateVideoFilter', 'durationUnit', value)"
        >
          <el-option :label="t('秒')" value="second" />
          <el-option :label="t('分钟')" value="minute" />
          <el-option :label="t('小时')" value="hour" />
        </el-select>
      </div>

      <div class="filter-control time-control">
        <span class="filter-label">{{ t('最近') }}</span>
        <el-button-group>
          <el-button
            v-for="days in [1, 3, 7, 30]"
            :key="days"
            size="small"
            :type="props.videoFilters.recentDays === days ? 'primary' : 'default'"
            @click="emit('updateVideoFilter', 'recentDays', days)"
          >{{ days }}{{ t('天') }}</el-button>
        </el-button-group>
        <el-input-number
          :model-value="props.videoFilters.recentDays"
          :min="1"
          :precision="0"
          size="small"
          controls-position="right"
          :placeholder="t('自定义')"
          @update:model-value="(value: number | undefined) => emit('updateVideoFilter', 'recentDays', value ?? null)"
        />
        <el-button
          v-if="props.videoFilters.recentDays !== null"
          size="small"
          text
          @click="emit('updateVideoFilter', 'recentDays', null)"
        >{{ t('不限') }}</el-button>
      </div>

      <div class="filter-summary">
        <span>{{ t('匹配') }} {{ props.tableVideos.length }}/{{ props.videoTotalCount }}</span>
        <span class="selected-count">{{ t('已选') }} {{ props.selectedVideoPaths.length }}</span>
        <span v-if="props.videoFilterMissingCount" class="missing-count">
          {{ t('元数据缺失') }} {{ props.videoFilterMissingCount }}
        </span>
        <el-button size="small" text type="primary" @click="emit('resetVideoFilters')">{{ t('重置筛选') }}</el-button>
      </div>
    </div>

    <div class="table-header-bar">
      <div class="tab-switch">
        <el-segmented
          :model-value="props.fileTypeTab"
          :options="[
            { label: `${t('视频')} (${props.tableVideos.length}/${props.videoTotalCount})`, value: 'video' },
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
        :data="videoPagination.rows"
        size="small"
        class="table-shell"
        height="100%"
        row-key="id"
        @sort-change="(payload: any) => emit('sortChange', 'video', { prop: payload?.prop || null, order: payload?.order || null })"
      >
        <el-table-column width="46" align="center">
          <template #header>
            <el-checkbox
              :model-value="allVisibleSelected"
              :indeterminate="someVisibleSelected"
              @update:model-value="(value: any) => emit('toggleAllVideoSelection', Boolean(value))"
            />
          </template>
          <template #default="{ row }">
            <el-checkbox
              :model-value="selectedVideoPathSet.has(row.path)"
              @update:model-value="(value: any) => emit('toggleVideoSelection', row.path, Boolean(value))"
            />
          </template>
        </el-table-column>
        <el-table-column width="72" align="center">
          <template #header><span>{{ t('序号') }}</span></template>
          <template #default="{ row, $index }">
            <div class="index-with-delete">
              <el-button type="danger" link :icon="Delete" @click="emit('remove', 'video', row.id)" />
              <span>{{ (videoPage - 1) * TABLE_PAGE_SIZE + $index + 1 }}</span>
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
                  <el-button text type="primary" size="small" @click="emit('inspect', row.path)">{{ t('查看详情') }}</el-button>
                </div>
              </div>
            </el-popover>
          </template>
        </el-table-column>
        <el-table-column :label="t('格式')" width="72" align="center">
          <template #default="{ row }">{{ extensionOf(row.name) }}</template>
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
      <div v-if="props.tableVideos.length > TABLE_PAGE_SIZE" class="table-pagination">
        <el-pagination
          :current-page="videoPagination.page"
          :page-size="TABLE_PAGE_SIZE"
          :total="props.tableVideos.length"
          layout="total, prev, pager, next"
          small
          background
          @update:current-page="(page: number) => (videoPage = page)"
        />
      </div>
      <div v-if="!hasData('video')" class="dropzone">
        <el-icon :size="40" class="dropzone-icon"><Upload /></el-icon>
        <p class="dropzone-title">{{ t('暂无数据') }}</p>
        <p class="dropzone-hint">{{ t('点击上方「添加」或「文件夹」，或粘贴路径导入') }}</p>
      </div>
    </div>

    <!-- Image Table -->
    <div v-if="props.fileTypeTab === 'image'" class="table-block" :class="{ 'is-empty': !hasData('image') }">
      <el-table
        v-if="hasData('image')"
        :data="imagePagination.rows"
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
              <span>{{ (imagePage - 1) * TABLE_PAGE_SIZE + $index + 1 }}</span>
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
      <div v-if="props.tableImages.length > TABLE_PAGE_SIZE" class="table-pagination">
        <el-pagination
          :current-page="imagePagination.page"
          :page-size="TABLE_PAGE_SIZE"
          :total="props.tableImages.length"
          layout="total, prev, pager, next"
          small
          background
          @update:current-page="(page: number) => (imagePage = page)"
        />
      </div>
      <div v-if="!hasData('image')" class="dropzone">
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
.video-filter-bar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px 10px;
  padding: 8px;
  border: 1px solid var(--border-secondary);
  border-radius: var(--radius-sm);
  background: var(--bg-secondary);
  flex-shrink: 0;
}
.filter-control {
  display: flex;
  align-items: center;
  gap: 5px;
}
.filter-label {
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 600;
  white-space: nowrap;
}
.format-control :deep(.el-select) {
  width: 130px;
}
.duration-control > :deep(.el-select) {
  width: 92px;
}
.duration-control :deep(.el-input-number) {
  width: 100px;
}
.duration-control :deep(.unit-select) {
  width: 74px;
}
.time-control :deep(.el-input-number) {
  width: 90px;
}
.range-separator {
  color: var(--text-muted);
}
.filter-summary {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
  color: var(--text-secondary);
  font-size: 11px;
  white-space: nowrap;
}
.selected-count {
  color: var(--accent);
  font-weight: 700;
}
.missing-count {
  color: var(--warning);
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
.table-pagination {
  display: flex;
  justify-content: flex-end;
  padding: 7px 10px;
  border-top: 1px solid var(--border-secondary);
  background: var(--bg-secondary);
  flex-shrink: 0;
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
