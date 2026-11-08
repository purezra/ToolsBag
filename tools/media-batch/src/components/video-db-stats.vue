<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { loadVideoStats, type StatBucket, type VideoDbStats } from '../api/video-db'
import { formatBytes, formatDuration } from '@core/utils/format'
import { useSettings } from '@core/hooks/useSettings'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', value: boolean): void }>()

const { t } = useSettings()
const visible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
})

const stats = ref<VideoDbStats | null>(null)
const loading = ref(false)
const lastUpdated = ref<Date | null>(null)

const refresh = async () => {
  loading.value = true
  try {
    stats.value = await loadVideoStats()
    lastUpdated.value = new Date()
  } catch (error: any) {
    ElMessage.error(`${t('读取数据库统计失败')}：${error?.message || error}`)
  } finally {
    loading.value = false
  }
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) void refresh()
  }
)

const sections = computed(() =>
  stats.value
    ? [
        { key: 'hdr', title: t('HDR 格式分布'), buckets: stats.value.byHdr },
        { key: 'resolution', title: t('分辨率分布'), buckets: stats.value.byResolution },
        { key: 'size', title: t('文件大小分布'), buckets: stats.value.bySize },
        { key: 'format', title: t('封装格式分布'), buckets: stats.value.byFormat },
        { key: 'codec', title: t('视频编码分布'), buckets: stats.value.byCodec },
        { key: 'depth', title: t('位深分布'), buckets: stats.value.byBitDepth },
      ]
    : []
)

const maxCount = (buckets: StatBucket[]) => Math.max(1, ...buckets.map((bucket) => bucket.count))

const barWidth = (bucket: StatBucket, buckets: StatBucket[]) =>
  `${Math.max(2, (bucket.count / maxCount(buckets)) * 100)}%`

const share = (count: number) => {
  const total = stats.value?.total ?? 0
  return total ? `${((count / total) * 100).toFixed(1)}%` : '0%'
}

const totalDuration = computed(() => formatDuration((stats.value?.totalDurationMs ?? 0) / 1000, 'clock'))
const updatedText = computed(() =>
  lastUpdated.value ? lastUpdated.value.toLocaleTimeString() : ''
)
</script>

<template>
  <el-dialog
    v-model="visible"
    :title="t('数据库统计')"
    width="clamp(680px, 88vw, 1080px)"
    top="6vh"
    append-to-body
  >
    <div v-loading="loading" class="db-stats">
      <div v-if="!stats" class="empty-block">
        <p>{{ t('正在读取数据库...') }}</p>
      </div>

      <div v-else-if="stats.total === 0" class="empty-block">
        <div class="empty-icon">🗂️</div>
        <p class="empty-title">{{ t('数据库中还没有视频记录') }}</p>
        <p class="empty-desc">{{ t('在「视频元数据导出」里导入后点击入库，或使用命令行 tools v 文件夹') }}</p>
      </div>

      <template v-else>
        <div class="tiles">
          <div class="tile tile-accent">
            <span class="tile-label">{{ t('视频总数') }}</span>
            <strong class="tile-value">{{ stats.total }}</strong>
          </div>
          <div class="tile">
            <span class="tile-label">{{ t('成功解析') }}</span>
            <strong class="tile-value">{{ stats.success }}</strong>
          </div>
          <div class="tile" :class="{ 'tile-warning': stats.failed > 0 }">
            <span class="tile-label">{{ t('解析失败') }}</span>
            <strong class="tile-value">{{ stats.failed }}</strong>
          </div>
          <div class="tile">
            <span class="tile-label">{{ t('合计大小') }}</span>
            <strong class="tile-value">{{ formatBytes(stats.totalSize) }}</strong>
          </div>
          <div class="tile">
            <span class="tile-label">{{ t('合计时长') }}</span>
            <strong class="tile-value">{{ totalDuration }}</strong>
          </div>
        </div>

        <div class="sections">
          <section v-for="section in sections" :key="section.key" class="section">
            <h4 class="section-title">{{ section.title }}</h4>
            <p v-if="!section.buckets.length" class="section-empty">{{ t('暂无数据') }}</p>
            <div v-else class="bucket-list">
              <div
                v-for="bucket in section.buckets"
                :key="bucket.label"
                class="bucket-row"
                :title="`${bucket.label} · ${bucket.count} ${t('个')} · ${formatBytes(bucket.size)}`"
              >
                <span class="bucket-label">{{ bucket.label }}</span>
                <div class="bucket-bar">
                  <div class="bucket-fill" :style="{ width: barWidth(bucket, section.buckets) }"></div>
                </div>
                <span class="bucket-count">{{ bucket.count }}</span>
                <span class="bucket-share">{{ share(bucket.count) }}</span>
              </div>
            </div>
          </section>
        </div>
      </template>
    </div>

    <template #footer>
      <span class="footer-note">
        video_records.db<template v-if="updatedText"> · {{ t('更新于') }} {{ updatedText }}</template>
      </span>
      <el-button size="small" :loading="loading" @click="refresh">{{ t('刷新') }}</el-button>
      <el-button size="small" type="primary" @click="visible = false">{{ t('关闭') }}</el-button>
    </template>
  </el-dialog>
</template>

<style scoped>
.db-stats {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-height: 70vh;
  overflow-y: auto;
  padding-right: 4px;
}

.tiles {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 10px;
}

.tile {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  border: 1px solid var(--border-secondary);
  border-radius: 10px;
  background: var(--bg-tertiary);
}

.tile-accent {
  background: var(--accent-light);
  border-color: color-mix(in srgb, var(--accent) 25%, var(--border-secondary));
}

.tile-warning {
  background: var(--warning-light);
}

.tile-label {
  font-size: 12px;
  color: var(--text-secondary);
}

.tile-value {
  font-size: 20px;
  line-height: 1.15;
  color: var(--text-primary);
  font-variant-numeric: tabular-nums;
}

.sections {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 14px 20px;
}

.section {
  min-width: 0;
}

.section-title {
  margin: 0 0 8px;
  font-size: 13px;
  font-weight: 700;
  color: var(--text-primary);
}

.section-empty {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
}

.bucket-list {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.bucket-row {
  display: grid;
  grid-template-columns: minmax(72px, 132px) minmax(60px, 1fr) 44px 52px;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.bucket-label {
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bucket-bar {
  height: 8px;
  border-radius: 4px;
  background: var(--bg-tertiary);
  overflow: hidden;
}

.bucket-fill {
  height: 100%;
  border-radius: 4px;
  background: var(--accent);
  transition: width 0.2s ease;
}

.bucket-count {
  text-align: right;
  font-weight: 700;
  color: var(--text-primary);
  font-variant-numeric: tabular-nums;
}

.bucket-share {
  text-align: right;
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}

.empty-block {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 40px 16px;
  text-align: center;
  color: var(--text-muted);
}

.empty-icon {
  font-size: 34px;
}

.empty-title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
}

.empty-desc {
  margin: 0;
  font-size: 12px;
}

.footer-note {
  float: left;
  font-size: 12px;
  color: var(--text-muted);
  line-height: 32px;
}
</style>
