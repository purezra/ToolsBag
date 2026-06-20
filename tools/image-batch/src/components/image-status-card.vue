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
        <div class="tb-stat-value" style="font-size: 14px;">{{ props.resultInfo.pageSize }}</div>
        <div class="tb-stat-label">页面尺寸</div>
      </div>
      <div class="tb-stat-item">
        <div class="tb-stat-value" style="font-size: 16px;">{{ props.resultInfo.inputSize }}</div>
        <div class="tb-stat-label">输入大小</div>
      </div>
      <div class="tb-stat-item">
        <div class="tb-stat-value" style="font-size: 16px;">{{ props.resultInfo.outputSize }}</div>
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
    <div class="tb-result-path" style="margin-top: 12px;">
      <el-button text :icon="LinkIcon" size="small" @click="emit('openOutput', props.resultInfo!.path)">
        打开所在文件夹
      </el-button>
      <span style="margin-left: 8px;">{{ props.resultInfo.path }}</span>
    </div>
  </div>

  <!-- 问题日志 -->
  <div v-if="props.problemLog && props.problemLog !== '暂无问题'" class="tb-section">
    <div class="tb-section-title" style="color: var(--warning);">
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
.flow-progress :deep(.el-progress-bar__inner) {
  background: linear-gradient(120deg, #6dd5ed, #2193b0, #6dd5ed);
  background-size: 200% 200%;
  animation: flow-bar 1.2s linear infinite;
}
@keyframes flow-bar {
  0% { background-position: 0% 50%; }
  100% { background-position: 200% 50%; }
}
.log-box {
  padding: 10px;
  min-height: 48px;
  white-space: pre-wrap;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-secondary);
  border-radius: var(--radius-sm);
  font-family: 'JetBrains Mono', Consolas, monospace;
  font-size: 12px;
  line-height: 1.5;
  color: var(--text-primary);
}
</style>
