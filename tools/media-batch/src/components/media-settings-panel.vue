<script setup lang="ts">
import { onMounted, ref } from 'vue'
import type { MediaKind, RenameField, ExternalToolStatus, MediaInfoStatus, RenameSafetySummary } from '@media-batch/types/media'
import type { DurationFormat } from '@core/utils/format'
import { useSettings } from '@core/hooks/useSettings'
import { checkFfprobeStatus, checkExiftoolStatus } from '../api/media-batch'

type ColumnsState = { duration?: boolean; resolution?: boolean; bitrate?: boolean; frameRate?: boolean; size?: boolean; preview?: boolean; device?: boolean; takenAt?: boolean; focalLength?: boolean }

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
}>()

const emit = defineEmits<{
  (e: 'update:durationFormat', value: DurationFormat): void
  (e: 'update:customText', value: string): void
  (e: 'update:separator', value: string): void
  (e: 'update:leadingZeros', value: number): void
  (e: 'toggleColumns', kind: MediaKind, field?: string, value?: boolean): void
  (e: 'moveField', list: RenameField[], index: number, direction: 'up' | 'down'): void
  (e: 'previewRename'): void
  (e: 'applyRename'): void
  (e: 'undoRename'): void
  (e: 'applyPreset', preset: 'short-video' | 'archive-video' | 'photo-exif'): void
  (e: 'toggleRenameFields', kind: MediaKind): void
}>()

const { t } = useSettings()

// 工具状态
const ffprobeStatus = ref<ExternalToolStatus>({ name: 'ffprobe', available: false })
const exiftoolStatus = ref<ExternalToolStatus>({ name: 'exiftool', available: false })
const checkingTools = ref(false)

const checkToolsStatus = async () => {
  checkingTools.value = true
  try {
    const [ff, ex] = await Promise.all([checkFfprobeStatus(), checkExiftoolStatus()])
    ffprobeStatus.value = ff
    exiftoolStatus.value = ex
  } catch (e) {
    console.error('检测工具状态失败:', e)
  } finally {
    checkingTools.value = false
  }
}

onMounted(() => {
  checkToolsStatus()
})
</script>

<template>
  <div class="settings">
    <!-- 工具状态检测 -->
    <div class="panel-header">
      <h3>{{ t('外部工具状态') }}</h3>
      <el-button size="small" :loading="checkingTools" @click="checkToolsStatus">
        {{ t('刷新') }}
      </el-button>
    </div>
    <div class="tools-status">
      <div class="tool-item">
        <span class="tool-name">MediaInfo.dll</span>
        <el-tag :type="props.mediaInfoStatus.available ? 'success' : 'danger'" size="small">
          {{ props.mediaInfoStatus.available ? t('可用') : t('未找到') }}
        </el-tag>
        <span v-if="props.mediaInfoStatus.path" class="tool-path" :title="props.mediaInfoStatus.path">
          {{ props.mediaInfoStatus.path }}
        </span>
      </div>
      <div class="tool-item">
        <span class="tool-name">ffprobe</span>
        <el-tag :type="ffprobeStatus.available ? 'success' : 'warning'" size="small">
          {{ ffprobeStatus.available ? (ffprobeStatus.version || t('可用')) : t('未安装') }}
        </el-tag>
        <span v-if="ffprobeStatus.path" class="tool-path" :title="ffprobeStatus.path">
          {{ ffprobeStatus.path }}
        </span>
      </div>
      <div class="tool-item">
        <span class="tool-name">exiftool</span>
        <el-tag :type="exiftoolStatus.available ? 'success' : 'warning'" size="small">
          {{ exiftoolStatus.available ? (exiftoolStatus.version || t('可用')) : t('未安装') }}
        </el-tag>
        <span v-if="exiftoolStatus.path" class="tool-path" :title="exiftoolStatus.path">
          {{ exiftoolStatus.path }}
        </span>
      </div>
    </div>

    <el-divider />

    <div class="panel-header">
      <div>
        <h3>{{ t('整理模板') }}</h3>
      </div>
    </div>
    <div class="preset-grid">
      <el-button size="small" plain @click="emit('applyPreset', 'short-video')">{{ t('短视频素材') }}</el-button>
      <el-button size="small" plain @click="emit('applyPreset', 'archive-video')">{{ t('归档命名') }}</el-button>
      <el-button size="small" plain @click="emit('applyPreset', 'photo-exif')">{{ t('图片 EXIF 整理') }}</el-button>
    </div>

    <el-divider />

    <div class="panel-header">
      <div>
        <h3>{{ t('重命名配置') }}</h3>
      </div>
    </div>
    <div class="form-grid">
      <template v-if="props.fileTypeTab === 'video'">
        <div class="form-item">
          <span>{{ t('时长格式') }}</span>
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
            <span>{{ t('视频列显示') }}</span>
            <el-button size="small" type="primary" plain @click="emit('toggleColumns', 'video')">{{ t('全选/全关') }}</el-button>
          </div>
          <div class="checkbox-grid">
            <el-checkbox :model-value="props.visibleVideoColumns.duration" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'duration', !!v)">{{ t('时长') }}</el-checkbox>
            <el-checkbox :model-value="props.visibleVideoColumns.resolution" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'resolution', !!v)">{{ t('分辨率') }}</el-checkbox>
            <el-checkbox :model-value="props.visibleVideoColumns.bitrate" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'bitrate', !!v)">{{ t('码率') }}</el-checkbox>
            <el-checkbox :model-value="props.visibleVideoColumns.frameRate" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'frameRate', !!v)">{{ t('帧率') }}</el-checkbox>
            <el-checkbox :model-value="props.visibleVideoColumns.size" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'size', !!v)">{{ t('文件大小') }}</el-checkbox>
            <el-checkbox :model-value="props.visibleVideoColumns.preview" @update:model-value="(v: any) => emit('toggleColumns', 'video', 'preview', !!v)">{{ t('预览名称') }}</el-checkbox>
          </div>
        </div>
        <div class="form-item">
          <div class="label-row">
            <span>{{ t('重命名字段（视频）') }}</span>
            <el-button size="small" type="primary" plain @click="emit('toggleRenameFields', 'video')">{{ t('全选/全关') }}</el-button>
          </div>
          <div class="rename-fields">
            <div v-for="(field, index) in props.renameFieldsVideo" :key="field.key" class="rename-row">
              <el-checkbox v-model="field.enabled">{{ t(field.label) }}</el-checkbox>
              <div class="row-actions">
                <el-button link type="primary" size="small" @click="emit('moveField', props.renameFieldsVideo, index, 'up')">{{ t('上移') }}</el-button>
                <el-button link type="primary" size="small" @click="emit('moveField', props.renameFieldsVideo, index, 'down')">{{ t('下移') }}</el-button>
              </div>
            </div>
          </div>
        </div>
      </template>

      <template v-else>
        <div class="form-item">
          <div class="label-row">
            <span>{{ t('图片列显示') }}</span>
            <el-button size="small" type="primary" plain @click="emit('toggleColumns', 'image')">{{ t('全选/全关') }}</el-button>
          </div>
          <div class="checkbox-grid">
            <el-checkbox v-model="props.visibleImageColumns.resolution">{{ t('分辨率') }}</el-checkbox>
            <el-checkbox v-model="props.visibleImageColumns.device">{{ t('拍摄设备') }}</el-checkbox>
            <el-checkbox v-model="props.visibleImageColumns.takenAt">{{ t('拍摄时间') }}</el-checkbox>
            <el-checkbox v-model="props.visibleImageColumns.focalLength">{{ t('焦距') }}</el-checkbox>
            <el-checkbox v-model="props.visibleImageColumns.size">{{ t('文件大小') }}</el-checkbox>
            <el-checkbox v-model="props.visibleImageColumns.preview">{{ t('预览名称') }}</el-checkbox>
          </div>
        </div>
        <div class="form-item">
          <div class="label-row">
            <span>{{ t('重命名字段（图片）') }}</span>
            <el-button size="small" type="primary" plain @click="emit('toggleRenameFields', 'image')">{{ t('全选/全关') }}</el-button>
          </div>
          <div class="rename-fields">
            <div v-for="(field, index) in props.renameFieldsImage" :key="field.key" class="rename-row">
              <el-checkbox v-model="field.enabled">{{ t(field.label) }}</el-checkbox>
              <div class="row-actions">
                <el-button link type="primary" size="small" @click="emit('moveField', props.renameFieldsImage, index, 'up')">{{ t('上移') }}</el-button>
                <el-button link type="primary" size="small" @click="emit('moveField', props.renameFieldsImage, index, 'down')">{{ t('下移') }}</el-button>
              </div>
            </div>
          </div>
        </div>
      </template>

      <div class="form-item inline">
        <span>{{ t('自定义文本') }}</span>
        <el-input :model-value="props.customText" size="small" placeholder="例：ProjectX" @input="(val: any) => emit('update:customText', val as string)" />
      </div>
      <div class="form-item inline">
        <span>{{ t('分隔符') }}</span>
        <el-input :model-value="props.separator" size="small" placeholder="_" style="max-width: 120px" @input="(val: any) => emit('update:separator', val as string)" />
      </div>
      <div class="form-item inline">
        <span>{{ t('序号补零') }}</span>
        <el-input-number :model-value="props.leadingZeros" size="small" :min="1" :max="4" @change="(val: any) => emit('update:leadingZeros', Number(val))" />
      </div>
    </div>

    <div class="rename-section">
      <span class="rename-label">{{ t('重命名') }}</span>
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
        <el-button round plain type="primary" class="large-btn" @click="emit('previewRename')">{{ t('预览') }}</el-button>
        <el-button round plain type="success" class="large-btn" @click="emit('applyRename')">{{ t('应用') }}</el-button>
        <el-button round plain type="warning" class="large-btn" :disabled="!props.canUndoRename" @click="emit('undoRename')">{{ t('撤销') }}</el-button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.tools-status {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  background: #f5f7fa;
  border-radius: 8px;
}
.tool-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}
.tool-name {
  min-width: 100px;
  font-weight: 500;
  color: #303133;
}
.tool-path {
  flex: 1;
  font-size: 11px;
  color: #909399;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.large-btn {
  flex: 1;
  height: 38px;
  font-size: 14px;
  font-weight: 600;
}
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}
.form-grid {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.preset-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}
.form-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.form-item.inline {
  flex-direction: row;
  align-items: center;
  justify-content: space-between;
}
.rename-fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px;
  border-radius: 10px;
  background: #f9fbff;
  border: 1px dashed rgba(20, 23, 31, 0.08);
}
.rename-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.row-actions {
  display: flex;
  gap: 4px;
}
.checkbox-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 6px;
}
.rename-section {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
}
.rename-label {
  font-size: 13px;
  font-weight: 600;
  color: #4a536a;
}
.rename-buttons {
  display: flex;
  justify-content: center;
  width: 100%;
  gap: 10px;
}
.rename-safety-card {
  width: 100%;
  box-sizing: border-box;
  padding: 10px;
  border-radius: 8px;
  border: 1px solid rgba(64, 158, 255, 0.18);
  background: #f5f9ff;
}
.safety-title {
  margin-bottom: 6px;
  font-size: 12px;
  font-weight: 700;
  color: #2f73ff;
}
.safety-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 4px 8px;
  font-size: 12px;
  color: #5f6678;
}
.safety-grid .warn {
  color: #d9901d;
  font-weight: 700;
}
.label-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
</style>
