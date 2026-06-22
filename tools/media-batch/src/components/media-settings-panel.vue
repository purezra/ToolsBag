<script setup lang="ts">
import { ref } from 'vue'
import type { MediaKind, RenameField, MediaInfoStatus, RenameSafetySummary } from '@media-batch/types/media'
import type { DurationFormat } from '@core/utils/format'
import { useSettings } from '@core/hooks/useSettings'
import { Rank } from '@element-plus/icons-vue'

type ColumnsState = { duration?: boolean; resolution?: boolean; bitrate?: boolean; size?: boolean; preview?: boolean; device?: boolean; takenAt?: boolean; focalLength?: boolean }

const props = defineProps<{
  fileTypeTab: MediaKind
  mediaInfoStatus: MediaInfoStatus
  durationFormat: DurationFormat
  renameFieldsVideo: RenameField[]
  renameFieldsImage: RenameField[]
  customText: string
  separator: string
  leadingZeros: number
  visibleVideoColumns: ColumnsState
  visibleImageColumns: ColumnsState
  showPreview: boolean
  renameSafetySummary: RenameSafetySummary
  canUndoRename: boolean
  undoStackDepth: number
}>()

const emit = defineEmits<{
  (e: 'update:durationFormat', value: DurationFormat): void
  (e: 'update:customText', value: string): void
  (e: 'update:separator', value: string): void
  (e: 'update:leadingZeros', value: number): void
  (e: 'toggleColumns', kind: MediaKind, field?: string, value?: boolean): void
  (e: 'toggleRenameField', kind: MediaKind, index: number, enabled: boolean): void
  (e: 'moveField', list: RenameField[], index: number, direction: 'up' | 'down'): void
  (e: 'previewRename'): void
  (e: 'applyRename'): void
  (e: 'undoRename'): void
  (e: 'applyPreset', preset: 'short-video' | 'archive-video' | 'photo-exif'): void
  (e: 'toggleRenameFields', kind: MediaKind): void
}>()

const { t } = useSettings()
const activeSection = ref('tools')

let dragIndex = ref<number | null>(null)

const onDragStart = (index: number) => {
  dragIndex.value = index
}

const onDragOver = (e: DragEvent) => {
  e.preventDefault()
}

const onDrop = (kind: 'video' | 'image', targetIndex: number) => {
  if (dragIndex.value === null || dragIndex.value === targetIndex) return
  const fields = kind === 'video' ? props.renameFieldsVideo : props.renameFieldsImage
  const from = dragIndex.value
  if (from < targetIndex) {
    for (let i = from; i < targetIndex; i++) {
      emit('moveField', fields, i, 'down')
    }
  } else {
    for (let i = from; i > targetIndex; i--) {
      emit('moveField', fields, i, 'up')
    }
  }
  dragIndex.value = null
}
</script>

<template>
  <div class="settings">
    <el-collapse v-model="activeSection" accordion>
      <el-collapse-item name="tools" :title="t('外部工具状态')">
        <div class="tools-status">
          <div class="tool-item">
            <span :class="['status-dot', props.mediaInfoStatus.available ? 'is-ok' : 'is-err']" />
            <span class="tool-name">MediaInfo</span>
            <span :class="['tool-status', props.mediaInfoStatus.available ? 'text-ok' : 'text-err']">
              {{ props.mediaInfoStatus.available ? t('可用') : t('未找到') }}
            </span>
            <span v-if="props.mediaInfoStatus.path" class="tool-path" :title="props.mediaInfoStatus.path">
              {{ props.mediaInfoStatus.path }}
            </span>
          </div>
        </div>
      </el-collapse-item>

      <el-collapse-item name="presets" :title="t('整理模板')">
        <div class="preset-grid">
          <el-button size="small" plain @click="emit('applyPreset', 'short-video')">{{ t('短视频素材') }}</el-button>
          <el-button size="small" plain @click="emit('applyPreset', 'archive-video')">{{ t('归档命名') }}</el-button>
          <el-button size="small" plain @click="emit('applyPreset', 'photo-exif')">{{ t('图片EXIF') }}</el-button>
        </div>
      </el-collapse-item>

      <el-collapse-item name="rename" :title="t('重命名配置')">
        <div class="form-grid">
          <template v-if="props.fileTypeTab === 'video'">
            <div class="form-item">
              <span class="form-label">{{ t('时长格式') }}</span>
              <el-segmented
                :model-value="props.durationFormat"
                :options="[
                  { label: 'h2m3s', value: 'hms' },
                  { label: '01:02:03', value: 'clock' },
                  { label: '12.3456min', value: 'minutes' }
                ]"
                size="small"
                @change="(val: any) => emit('update:durationFormat', val)"
              />
            </div>
            <div class="form-item">
              <div class="label-row">
                <span class="form-label">{{ t('视频列显示') }}</span>
                <el-button size="small" text type="primary" @click="emit('toggleColumns', 'video')">{{ t('全选/全关') }}</el-button>
              </div>
              <div class="checkbox-grid">
                <el-checkbox :model-value="props.visibleVideoColumns.duration" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'duration', !!v)">{{ t('时长') }}</el-checkbox>
                <el-checkbox :model-value="props.visibleVideoColumns.resolution" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'resolution', !!v)">{{ t('分辨率') }}</el-checkbox>
                <el-checkbox :model-value="props.visibleVideoColumns.bitrate" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'bitrate', !!v)">{{ t('码率') }}</el-checkbox>
                <el-checkbox :model-value="props.visibleVideoColumns.size" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'size', !!v)">{{ t('文件大小') }}</el-checkbox>
                <el-checkbox :model-value="props.visibleVideoColumns.preview" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'preview', !!v)">{{ t('预览名称') }}</el-checkbox>
              </div>
            </div>
            <div class="form-item">
              <div class="label-row">
                <span class="form-label">{{ t('重命名字段') }}</span>
                <el-button size="small" text type="primary" @click="emit('toggleRenameFields', 'video')">{{ t('全选/全关') }}</el-button>
              </div>
              <div class="rename-fields">
                <div
                  v-for="(field, index) in props.renameFieldsVideo"
                  :key="field.key"
                  class="rename-row"
                  draggable="true"
                  @dragstart="onDragStart(index)"
                  @dragover="onDragOver"
                  @drop="onDrop('video', index)"
                >
                  <el-icon class="drag-handle" :size="14"><Rank /></el-icon>
                  <el-checkbox :model-value="field.enabled" @update:model-value="(v: any) => emit('toggleRenameField', 'video', index, !!v)">{{ t(field.label) }}</el-checkbox>
                </div>
              </div>
            </div>
          </template>

          <template v-else>
            <div class="form-item">
              <div class="label-row">
                <span class="form-label">{{ t('图片列显示') }}</span>
                <el-button size="small" text type="primary" @click="emit('toggleColumns', 'image')">{{ t('全选/全关') }}</el-button>
              </div>
              <div class="checkbox-grid">
                <el-checkbox :model-value="props.visibleImageColumns.resolution" @update:model-value="(v: any) => emit('toggleColumns', 'image', 'resolution', !!v)">{{ t('分辨率') }}</el-checkbox>
                <el-checkbox :model-value="props.visibleImageColumns.device" @update:model-value="(v: any) => emit('toggleColumns', 'image', 'device', !!v)">{{ t('拍摄设备') }}</el-checkbox>
                <el-checkbox :model-value="props.visibleImageColumns.takenAt" @update:model-value="(v: any) => emit('toggleColumns', 'image', 'takenAt', !!v)">{{ t('拍摄时间') }}</el-checkbox>
                <el-checkbox :model-value="props.visibleImageColumns.focalLength" @update:model-value="(v: any) => emit('toggleColumns', 'image', 'focalLength', !!v)">{{ t('焦距') }}</el-checkbox>
                <el-checkbox :model-value="props.visibleImageColumns.size" @update:model-value="(v: any) => emit('toggleColumns', 'image', 'size', !!v)">{{ t('文件大小') }}</el-checkbox>
                <el-checkbox :model-value="props.visibleImageColumns.preview" @update:model-value="(v: any) => emit('toggleColumns', 'image', 'preview', !!v)">{{ t('预览名称') }}</el-checkbox>
              </div>
            </div>
            <div class="form-item">
              <div class="label-row">
                <span class="form-label">{{ t('重命名字段') }}</span>
                <el-button size="small" text type="primary" @click="emit('toggleRenameFields', 'image')">{{ t('全选/全关') }}</el-button>
              </div>
              <div class="rename-fields">
                <div
                  v-for="(field, index) in props.renameFieldsImage"
                  :key="field.key"
                  class="rename-row"
                  draggable="true"
                  @dragstart="onDragStart(index)"
                  @dragover="onDragOver"
                  @drop="onDrop('image', index)"
                >
                  <el-icon class="drag-handle" :size="14"><Rank /></el-icon>
                  <el-checkbox :model-value="field.enabled" @update:model-value="(v: any) => emit('toggleRenameField', 'image', index, !!v)">{{ t(field.label) }}</el-checkbox>
                </div>
              </div>
            </div>
          </template>

          <div class="form-item inline">
            <span class="form-label">{{ t('自定义文本') }}</span>
            <el-input :model-value="props.customText" size="small" placeholder="ProjectX" @input="(val: any) => emit('update:customText', val as string)" />
          </div>
          <div class="form-item inline">
            <span class="form-label">{{ t('分隔符') }}</span>
            <el-input :model-value="props.separator" size="small" placeholder="_" style="max-width: 100px" @input="(val: any) => emit('update:separator', val as string)" />
          </div>
          <div class="form-item inline">
            <span class="form-label">{{ t('序号补零') }}</span>
            <el-input-number :model-value="props.leadingZeros" size="small" :min="1" :max="4" @change="(val: any) => emit('update:leadingZeros', Number(val))" />
          </div>
        </div>

        <div class="rename-section">
          <div v-if="props.showPreview" class="rename-safety-card">
            <div class="safety-title">{{ t('重命名预检') }}</div>
            <div class="safety-grid">
              <span>{{ t('待处理') }}: {{ props.renameSafetySummary.readyCount }}/{{ props.renameSafetySummary.total }}</span>
              <span>{{ t('不变') }}: {{ props.renameSafetySummary.unchangedCount }}</span>
              <span :class="{ warn: props.renameSafetySummary.illegalNameCount > 0 }">
                {{ t('非法字符') }}: {{ props.renameSafetySummary.illegalNameCount }}
              </span>
              <span :class="{ warn: props.renameSafetySummary.duplicateTargetCount > 0 }">
                {{ t('目标重名') }}: {{ props.renameSafetySummary.duplicateTargetCount }}
              </span>
            </div>
          </div>
          <div class="rename-buttons">
            <el-button plain type="primary" class="large-btn" @click="emit('previewRename')">{{ t('预览') }}</el-button>
            <el-button plain type="success" class="large-btn" @click="emit('applyRename')">{{ t('应用') }}</el-button>
            <el-button plain type="warning" class="large-btn" :disabled="!props.canUndoRename" @click="emit('undoRename')">{{ props.undoStackDepth > 1 ? t('撤销') + `(${props.undoStackDepth})` : t('撤销') }}</el-button>
          </div>
        </div>
      </el-collapse-item>
    </el-collapse>
  </div>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
}
.settings :deep(.el-collapse-item__header) {
  font-size: 12px;
  font-weight: 600;
  padding: 0 2px;
  height: 32px;
  line-height: 32px;
  border-bottom: none;
}
.settings :deep(.el-collapse-item__wrap) {
  border-bottom: none;
}
.settings :deep(.el-collapse-item__content) {
  padding-bottom: 8px;
}
.tools-status {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.tool-item {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  white-space: nowrap;
}
.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}
.status-dot.is-ok {
  background: var(--success);
  box-shadow: 0 0 5px var(--success);
}
.status-dot.is-warn {
  background: var(--warning);
  box-shadow: 0 0 5px var(--warning);
}
.status-dot.is-err {
  background: var(--danger);
  box-shadow: 0 0 5px var(--danger);
}
.tool-name {
  font-weight: 500;
  color: var(--text-primary);
}
.tool-status {
  font-size: 11px;
  font-weight: 500;
}
.text-ok { color: var(--success); }
.text-warn { color: var(--warning); }
.text-err { color: var(--danger); }
.tool-path {
  font-size: 10px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.preset-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 6px;
}
.form-grid {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.form-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.form-item.inline {
  flex-direction: row;
  align-items: center;
  justify-content: space-between;
}
.form-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
}
.label-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.checkbox-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 2px 4px;
}
.rename-fields {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 4px;
  border-radius: var(--radius-sm);
  background: var(--bg-tertiary);
  border: 1px dashed var(--border-primary);
}
.rename-row {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 2px 4px;
  border-radius: 4px;
  cursor: grab;
  transition: background var(--duration-fast) ease;
}
.rename-row:hover {
  background: var(--accent-light);
}
.rename-row:active {
  cursor: grabbing;
}
.drag-handle {
  color: var(--text-muted);
  flex-shrink: 0;
  cursor: grab;
}
.rename-row:active .drag-handle {
  cursor: grabbing;
}
.rename-section {
  margin-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.rename-buttons {
  display: flex;
  justify-content: center;
  width: 100%;
  gap: 8px;
}
.large-btn {
  flex: 1;
  height: 30px;
  font-size: 12px;
  font-weight: 600;
}
.rename-safety-card {
  width: 100%;
  box-sizing: border-box;
  padding: 6px 8px;
  border-radius: 6px;
  border: 1px solid var(--border-secondary);
  background: var(--bg-tertiary);
}
.safety-title {
  margin-bottom: 4px;
  font-size: 11px;
  font-weight: 700;
  color: var(--accent);
}
.safety-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 2px 6px;
  font-size: 11px;
  color: var(--text-secondary);
}
.safety-grid .warn {
  color: #d9901d;
  font-weight: 700;
}
</style>
