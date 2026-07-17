<script setup lang="ts">
import { Link as LinkIcon } from '@element-plus/icons-vue'

type ProgressState = { stage: string; percent: number; message: string }

type ResultInfo = {
  path: string
  inputSize: string
  outputSize: string
  deltaText: string
  increase: boolean
  pageCount?: number
  pageSize?: string
}

const props = defineProps<{
  progress: ProgressState | null
  resultInfo: ResultInfo | null
  problemLog: string
}>()

const emit = defineEmits<{
  (e: 'openOutput', path: string): void
}>()
</script>

<template>
  <!-- 转换进度 -->
  <div v-if="props.progress" class="tb-section">
    <div class="tb-section-title">{{ props.progress.stage }}</div>
    <el-progress :percentage="props.progress.percent" :text-inside="true" class="flow-progress" />
    <div class="progress-msg">{{ props.progress.message }}</div>
  </div>

  <!-- 转换结果 -->
  <div v-if="props.resultInfo" class="tb-section">
    <div class="tb-section-title">转换结果</div>

    <!-- 统计数据 -->
    <div class="tb-stat-grid">
      <div v-if="props.resultInfo.pageCount" class="tb-stat-item">
        <div class="tb-stat-value">{{ props.resultInfo.pageCount }}</div>
        <div class="tb-stat-label">页数</div>
      </div>
      <div v-if="props.resultInfo.pageSize" class="tb-stat-item">
        <div class="tb-stat-value tb-stat-value--xs">{{ props.resultInfo.pageSize }}</div>
        <div class="tb-stat-label">页面尺寸</div>
      </div>
      <div class="tb-stat-item">
        <div class="tb-stat-value tb-stat-value--sm">{{ props.resultInfo.inputSize }}</div>
        <div class="tb-stat-label">输入大小</div>
      </div>
      <div class="tb-stat-item">
        <div class="tb-stat-value tb-stat-value--sm">{{ props.resultInfo.outputSize }}</div>
        <div class="tb-stat-label">输出大小</div>
      </div>
      <div class="tb-stat-item">
        <div class="tb-delta" :class="props.resultInfo.increase ? 'tb-delta--up' : 'tb-delta--down'">
          {{ props.resultInfo.deltaText }}
        </div>
        <div class="tb-stat-label">体积变化</div>
      </div>
    </div>

    <!-- 输出路径 -->
    <div class="result-path-row">
      <el-button text :icon="LinkIcon" size="small" @click="emit('openOutput', props.resultInfo!.path)">
        打开所在文件夹
      </el-button>
      <span class="result-path-text">{{ props.resultInfo.path }}</span>
    </div>
  </div>

  <!-- 问题日志 -->
  <div v-if="props.problemLog && props.problemLog !== '暂无问题'" class="tb-section">
    <div class="tb-section-title is-warn">
      问题日志
      <el-tag size="small" type="warning">有异常</el-tag>
    </div>
    <pre class="log-box">{{ props.problemLog }}</pre>
  </div>
</template>

<style scoped>
.progress-msg {
  margin-top: 8px;
  color: var(--text-secondary);
  font-size: 13px;
}
/* Progress fill is handled globally by overrides.css (per-skin gradient);
   no hardcoded gradient here. */
.result-path-row {
  margin-top: 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-muted);
  word-break: break-all;
}
.tb-section-title.is-warn {
  color: var(--warning);
}
.log-box {
  padding: 10px;
  min-height: 48px;
  white-space: pre-wrap;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-secondary);
  border-radius: var(--radius-sm);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.5;
  color: var(--text-primary);
}
</style>
