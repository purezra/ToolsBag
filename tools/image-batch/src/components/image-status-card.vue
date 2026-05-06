<script setup lang="ts">
import { Link as LinkIcon } from '@element-plus/icons-vue'

type ProgressState = { stage: string; percent: number; message: string }

type ResultInfo = {
  path: string
  inputSize: string
  outputSize: string
  deltaText: string
  increase: boolean
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
  <div class="status-panel">
    <div v-if="props.progress" class="progress-box">
      <div class="progress-title">{{ props.progress.stage }}</div>
      <el-progress :percentage="props.progress.percent" :text-inside="true" class="flow-progress" />
      <div class="progress-msg">{{ props.progress.message }}</div>
    </div>

    <div v-if="props.resultInfo" class="result-card">
      <div class="result-left">
        <div class="label">输出位置</div>
        <div class="path">{{ props.resultInfo.path }}</div>
        <el-button text :icon="LinkIcon" @click="emit('openOutput', props.resultInfo.path)">打开所在文件夹</el-button>
      </div>
      <div class="result-right">
        <div class="size-line">输入总大小：{{ props.resultInfo.inputSize }}</div>
        <div class="size-line">输出总大小：{{ props.resultInfo.outputSize }}</div>
        <div class="size-delta" :class="{ up: props.resultInfo.increase, down: !props.resultInfo.increase }">
          {{ props.resultInfo.deltaText }}
        </div>
      </div>
    </div>

    <div class="log">
      <div class="log-title">问题日志</div>
      <pre class="log-box">{{ props.problemLog || '暂无问题' }}</pre>
    </div>
  </div>
</template>

<style scoped>
.status-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.progress-box {
  border: 1px solid #e5e5e5;
  padding: 10px;
  border-radius: 8px;
  background: #fafafa;
}
.progress-title {
  font-weight: 600;
  margin-bottom: 4px;
}
.progress-msg {
  margin-top: 4px;
  color: #666;
}
.flow-progress .el-progress-bar__inner {
  background: linear-gradient(120deg, #6dd5ed, #2193b0, #6dd5ed);
  background-size: 200% 200%;
  animation: flow-bar 1.2s linear infinite;
}
@keyframes flow-bar {
  0% {
    background-position: 0% 50%;
  }
  100% {
    background-position: 200% 50%;
  }
}
.result-card {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
  padding: 12px;
  border: 1px solid #e5e5e5;
  border-radius: 10px;
  background: #f8fbff;
}
.result-left .label {
  font-weight: 600;
  margin-bottom: 4px;
}
.path {
  font-family: Consolas, 'SFMono-Regular', monospace;
  font-size: 13px;
  color: #2c3e50;
  word-break: break-all;
}
.result-right {
  text-align: right;
}
.size-line {
  color: #444;
  margin-bottom: 2px;
}
.size-delta {
  font-weight: 700;
}
.size-delta.up {
  color: #e67e22;
}
.size-delta.down {
  color: #2ecc71;
}
.log {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.log-box {
  border: 1px solid #e5e5e5;
  background: #f8f8f8;
  padding: 8px;
  min-height: 120px;
  border-radius: 6px;
  white-space: pre-wrap;
}
</style>
