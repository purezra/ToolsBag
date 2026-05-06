<script setup lang="ts">
import { onMounted, ref } from 'vue'
import type { MediaKind, RenameField, ExternalToolStatus, MediaInfoStatus } from '@media-batch/types/media'
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
}>()

const emit = defineEmits<{
  (e: 'update:durationFormat', value: DurationFormat): void
  (e: 'update:customText', value: string): void
  (e: 'update:separator', value: string): void
  (e: 'update:leadingZeros', value: number): void
  (e: 'toggleColumns', kind: MediaKind): void
  (e: 'moveField', list: RenameField[], index: number, direction: 'up' | 'down'): void
  (e: 'previewRename'): void
  (e: 'applyRename'): void
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
            <el-checkbox v-model="props.visibleVideoColumns.duration">{{ t('时长') }}</el-checkbox>
            <el-checkbox v-model="props.visibleVideoColumns.resolution">{{ t('分辨率') }}</el-checkbox>
            <el-checkbox v-model="props.visibleVideoColumns.bitrate">{{ t('码率') }}</el-checkbox>
            <el-checkbox v-model="props.visibleVideoColumns.frameRate">{{ t('帧率') }}</el-checkbox>
            <el-checkbox v-model="props.visibleVideoColumns.size">{{ t('文件大小') }}</el-checkbox>
            <el-checkbox v-model="props.visibleVideoColumns.preview">{{ t('预览名称') }}</el-checkbox>
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
      <div class="rename-buttons">
        <el-button round plain type="primary" class="large-btn" @click="emit('previewRename')">{{ t('预览') }}</el-button>
        <el-button round plain type="success" class="large-btn" @click="emit('applyRename')">{{ t('应用') }}</el-button>
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
.label-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
</style>
