<script setup lang="ts">
import { computed } from 'vue'
import { ArrowDown, ArrowUp } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'
import { formatBytes } from '@core/utils/format'
import type { DisplayLevel, VideoInfoItem } from '../types/media'

const props = defineProps<{
  item: VideoInfoItem
  level: DisplayLevel
  expanded: boolean
}>()

const emit = defineEmits<{
  (e: 'toggle'): void
}>()

const { t } = useSettings()

const LEVEL_ORDER: DisplayLevel[] = ['public', 'beginner', 'advanced', 'professional']

const levelIndex = computed(() => LEVEL_ORDER.indexOf(props.level))
const show = (minLevel: DisplayLevel) => levelIndex.value >= LEVEL_ORDER.indexOf(minLevel)

const detail = computed(() => props.item.detail)

type FieldItem = { label: string; value: string; level: DisplayLevel }
const field = (label: string, value: string, level: DisplayLevel): FieldItem => ({ label, value, level })

const generalFields = computed<FieldItem[]>(() => {
  if (!detail.value) return []
  const g = detail.value.general
  return [
    field(t('封装格式'), g.format || '-', 'public'),
    field(t('格式详情'), g.formatLong || '-', 'beginner'),
    field(t('文件大小'), g.fileSizeStr || formatBytes(g.fileSize), 'public'),
    field(t('时长'), g.duration || '-', 'public'),
    field(t('总码率'), g.overallBitRate || '-', 'beginner'),
    field(t('标题'), g.title || '-', 'public'),
    field(t('编码时间'), g.encodedDate || '-', 'beginner'),
    field(t('封装工具'), g.writingApplication || '-', 'beginner'),
    field(t('编码标识'), g.codecId || '-', 'advanced'),
    field(t('编码库'), g.encodedLibrary || '-', 'advanced'),
    field(t('流数量'), g.streamCount ? String(g.streamCount) : '-', 'advanced'),
  ].filter(f => show(f.level))
})

const makeVideoFields = (stream: any): FieldItem[] => [
  field(t('分辨率'), stream.width && stream.height ? `${stream.width} × ${stream.height}` : '-', 'public'),
  field(t('画面比例'), stream.displayAspectRatio || '-', 'public'),
  field(t('帧率'), stream.frameRate || '-', 'public'),
  field(t('视频编码'), stream.codec || '-', 'beginner'),
  field(t('视频码率'), stream.bitRate || '-', 'beginner'),
  field(t('帧率模式'), stream.frameRateMode || '-', 'beginner'),
  field(t('位深'), stream.bitDepth || '-', 'beginner'),
  field(t('HDR格式'), stream.hdrFormat || '-', 'beginner'),
  field(t('扫描方式'), stream.scanType || '-', 'beginner'),
  field(t('编码档次'), stream.formatProfile || '-', 'advanced'),
  field(t('色度抽样'), stream.chromaSubsampling || '-', 'advanced'),
  field(t('色彩空间'), stream.colorSpace || '-', 'advanced'),
  field(t('色域'), stream.colorPrimaries || '-', 'advanced'),
  field(t('传递函数'), stream.transferCharacteristics || '-', 'advanced'),
  field(t('色彩矩阵'), stream.matrixCoefficients || '-', 'advanced'),
  field(t('流大小'), stream.streamSize || '-', 'advanced'),
  field(t('压缩效率'), stream.bitsPerPixelFrame || '-', 'advanced'),
  field(t('语言'), stream.language || '-', 'advanced'),
  field('CABAC', stream.cabac || '-', 'professional'),
  field(t('参考帧数'), stream.formatSettingsRefFrames || '-', 'professional'),
  field(t('编码库'), stream.encodedLibrary || '-', 'professional'),
  field(t('编码器参数'), stream.encodedLibrarySettings || '-', 'professional'),
  field(t('编码标识'), stream.codecId || '-', 'professional'),
  field(t('时长'), stream.duration || '-', 'professional'),
].filter(f => show(f.level))

const makeAudioFields = (stream: any): FieldItem[] => [
  field(t('声道数'), stream.channels || '-', 'public'),
  field(t('声道布局'), stream.channelLayout || '-', 'public'),
  field(t('采样率'), stream.sampleRate || '-', 'public'),
  field(t('音频编码'), stream.codec || '-', 'beginner'),
  field(t('音频码率'), stream.bitRate || '-', 'beginner'),
  field(t('码率模式'), stream.bitRateMode || '-', 'beginner'),
  field(t('默认音轨'), stream.isDefault ? t('是') : t('否'), 'beginner'),
  field(t('语言'), stream.language || '-', 'advanced'),
  field(t('标题'), stream.title || '-', 'advanced'),
  field(t('流大小'), stream.streamSize || '-', 'advanced'),
  field(t('格式档次'), stream.formatProfile || '-', 'advanced'),
  field(t('压缩模式'), stream.compressionMode || '-', 'advanced'),
  field(t('时长'), stream.duration || '-', 'advanced'),
  field(t('编码标识'), stream.codecId || '-', 'professional'),
].filter(f => show(f.level))

const makeTextFields = (stream: any): FieldItem[] => [
  field(t('字幕格式'), stream.format || '-', 'public'),
  field(t('语言'), stream.language || '-', 'public'),
  field(t('标题'), stream.title || '-', 'beginner'),
  field(t('编码标识'), stream.codecId || '-', 'advanced'),
  field(t('默认字幕'), stream.isDefault ? t('是') : t('否'), 'beginner'),
].filter(f => show(f.level))

const hasStreams = computed(() => {
  if (!detail.value) return false
  return detail.value.videoStreams.length > 0 || detail.value.audioStreams.length > 0 || detail.value.textStreams.length > 0
})

const videoStreams = computed(() => detail.value?.videoStreams || [])
const audioStreams = computed(() => detail.value?.audioStreams || [])
const textStreams = computed(() => detail.value?.textStreams || [])

const statusType = computed(() => props.item.status === 'success' ? 'success' : 'danger')
</script>

<template>
  <div class="detail-card" :class="{ 'is-expanded': expanded }">
    <!-- 文件头 -->
    <div class="card-header" @click="emit('toggle')">
      <div class="header-left">
        <el-tag :type="statusType" size="small" class="status-tag">
          {{ item.status === 'success' ? t('成功') : t('失败') }}
        </el-tag>
        <span class="file-name" :title="item.name">{{ item.name }}</span>
        <span class="file-size">{{ formatBytes(item.size) }}</span>
      </div>
      <div class="header-right">
        <span v-if="detail" class="stream-summary">
          <el-tag v-if="videoStreams.length" size="small" type="info">
            {{ t('视频') }} {{ videoStreams.length }}
          </el-tag>
          <el-tag v-if="audioStreams.length" size="small" type="info">
            {{ t('音频') }} {{ audioStreams.length }}
          </el-tag>
          <el-tag v-if="textStreams.length" size="small" type="info">
            {{ t('字幕') }} {{ textStreams.length }}
          </el-tag>
        </span>
        <el-icon class="expand-icon">
          <component :is="expanded ? ArrowUp : ArrowDown" />
        </el-icon>
      </div>
    </div>

    <!-- 错误信息 -->
    <div v-if="item.status !== 'success' && item.reason" class="card-error">
      {{ item.reason }}
    </div>

    <!-- 详细信息展开区 -->
    <div v-if="expanded && detail && hasStreams" class="card-body">
      <!-- 基础文件信息 -->
      <div v-if="generalFields.length" class="info-section">
        <div class="section-title">
          <span class="section-dot"></span>
          {{ t('基础文件信息') }}
        </div>
        <div class="field-grid">
          <div v-for="field in generalFields" :key="field.label" class="field-item">
            <span class="field-label">{{ field.label }}</span>
            <span class="field-value" :title="field.value">{{ field.value }}</span>
          </div>
        </div>
      </div>

      <!-- 视频流 -->
      <div v-for="(stream, sIdx) in videoStreams" :key="'v' + sIdx" class="info-section">
        <div class="section-title">
          <span class="section-dot video"></span>
          {{ t('视频流') }} {{ videoStreams.length > 1 ? `#${sIdx + 1}` : '' }}
          <el-tag v-if="stream.hdrFormat" size="small" type="warning" class="hdr-tag">HDR</el-tag>
          <el-tag v-if="stream.scanType === 'Interlaced'" size="small" type="info" class="hdr-tag">{{ t('隔行') }}</el-tag>
        </div>
        <div class="field-grid">
          <div v-for="field in makeVideoFields(stream)" :key="field.label" class="field-item">
            <span class="field-label">{{ field.label }}</span>
            <span class="field-value" :title="field.value">{{ field.value }}</span>
          </div>
        </div>
      </div>

      <!-- 音频流 -->
      <div v-for="(stream, aIdx) in audioStreams" :key="'a' + aIdx" class="info-section">
        <div class="section-title">
          <span class="section-dot audio"></span>
          {{ t('音频流') }} {{ audioStreams.length > 1 ? `#${aIdx + 1}` : '' }}
          <el-tag v-if="stream.isDefault" size="small" type="success" class="hdr-tag">{{ t('默认') }}</el-tag>
        </div>
        <div class="field-grid">
          <div v-for="field in makeAudioFields(stream)" :key="field.label" class="field-item">
            <span class="field-label">{{ field.label }}</span>
            <span class="field-value" :title="field.value">{{ field.value }}</span>
          </div>
        </div>
      </div>

      <!-- 字幕流 -->
      <div v-for="(stream, tIdx) in textStreams" :key="'t' + tIdx" class="info-section">
        <div class="section-title">
          <span class="section-dot text"></span>
          {{ t('字幕') }} {{ textStreams.length > 1 ? `#${tIdx + 1}` : '' }}
          <el-tag v-if="stream.isDefault" size="small" type="success" class="hdr-tag">{{ t('默认') }}</el-tag>
        </div>
        <div class="field-grid">
          <div v-for="field in makeTextFields(stream)" :key="field.label" class="field-item">
            <span class="field-label">{{ field.label }}</span>
            <span class="field-value" :title="field.value">{{ field.value }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.detail-card {
  border: 1px solid rgba(20, 23, 31, 0.06);
  border-radius: 12px;
  background: #fff;
  overflow: hidden;
  transition: box-shadow 0.2s;
}
.detail-card:hover {
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.06);
}
.detail-card.is-expanded {
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  cursor: pointer;
  user-select: none;
  background: #fafbfd;
  transition: background 0.15s;
}
.card-header:hover {
  background: #f0f4ff;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  flex: 1;
}
.status-tag {
  flex-shrink: 0;
}
.file-name {
  font-weight: 600;
  font-size: 13px;
  color: #1a1d26;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.file-size {
  font-size: 12px;
  color: #8b8fa3;
  flex-shrink: 0;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}
.stream-summary {
  display: flex;
  gap: 4px;
}
.expand-icon {
  color: #8b8fa3;
  font-size: 16px;
}

.card-error {
  padding: 8px 16px;
  background: #fef0f0;
  color: #f56c6c;
  font-size: 12px;
}

.card-body {
  padding: 0 16px 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.info-section {
  padding-top: 12px;
}
.info-section + .info-section {
  border-top: 1px solid rgba(20, 23, 31, 0.04);
}

.section-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  color: #303133;
  margin-bottom: 10px;
}
.section-dot {
  width: 4px;
  height: 16px;
  border-radius: 2px;
  background: #409eff;
}
.section-dot.video {
  background: #409eff;
}
.section-dot.audio {
  background: #67c23a;
}
.section-dot.text {
  background: #e6a23c;
}
.hdr-tag {
  margin-left: 4px;
}

.field-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 6px 16px;
}

.field-item {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 4px 0;
  min-width: 0;
}
.field-label {
  font-size: 12px;
  color: #8b8fa3;
  flex-shrink: 0;
  min-width: 70px;
}
.field-value {
  font-size: 12px;
  color: #1a1d26;
  font-family: 'JetBrainsMonoNL-Regular', monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  word-break: break-all;
}
</style>
