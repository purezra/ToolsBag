<script setup lang="ts">
import type { SyncConflict } from '@codebook/types/webdav'

defineProps<{
  conflicts: SyncConflict[]
}>()

const emit = defineEmits<{
  (e: 'resolve', entryId: string, keepLocal: boolean): void
  (e: 'close'): void
}>()

const formatTimestamp = (ts: number) => {
  return new Date(ts * 1000).toLocaleString('zh-CN')
}
</script>

<template>
  <div class="sync-conflict-dialog">
    <div class="header">
      <h3>发现同步冲突</h3>
      <button class="close-btn" @click="emit('close')">×</button>
    </div>

    <p class="desc">以下记录在多个设备上被同时修改，请选择保留的版本：</p>

    <div class="conflict-list">
      <div
        v-for="conflict in conflicts"
        :key="conflict.entryId"
        class="conflict-item"
      >
        <div class="entry-title">{{ conflict.entryTitle }}</div>
        <div class="versions">
          <div class="version local">
            <div class="label">本地版本</div>
            <div class="device">设备: {{ conflict.localUpdatedBy.slice(0, 8) }}...</div>
            <div class="time">{{ formatTimestamp(conflict.localUpdatedAt) }}</div>
            <button
              class="btn primary"
              @click="emit('resolve', conflict.entryId, true)"
            >
              保留本地
            </button>
          </div>
          <div class="vs">VS</div>
          <div class="version remote">
            <div class="label">远端版本</div>
            <div class="device">设备: {{ conflict.remoteUpdatedBy.slice(0, 8) }}...</div>
            <div class="time">{{ formatTimestamp(conflict.remoteUpdatedAt) }}</div>
            <button
              class="btn secondary"
              @click="emit('resolve', conflict.entryId, false)"
            >
              保留远端
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sync-conflict-dialog {
  background: var(--card-bg, #fff);
  border-radius: 12px;
  padding: 20px;
  max-width: 600px;
  width: 100%;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.header h3 {
  margin: 0;
  font-size: 18px;
  color: #f44336;
}

.close-btn {
  background: none;
  border: none;
  font-size: 24px;
  cursor: pointer;
  color: var(--text-secondary, #666);
}

.desc {
  color: var(--text-secondary, #666);
  font-size: 14px;
  margin-bottom: 16px;
}

.conflict-list {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-height: 400px;
  overflow-y: auto;
}

.conflict-item {
  border: 1px solid var(--border-color, #eee);
  border-radius: 10px;
  padding: 14px;
}

.entry-title {
  font-weight: 600;
  font-size: 16px;
  margin-bottom: 12px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-color, #eee);
}

.versions {
  display: flex;
  align-items: center;
  gap: 16px;
}

.version {
  flex: 1;
  padding: 12px;
  border-radius: 8px;
  text-align: center;
}

.version.local {
  background: #e3f2fd;
}

.version.remote {
  background: #fff3e0;
}

.version .label {
  font-weight: 600;
  margin-bottom: 8px;
}

.version .device,
.version .time {
  font-size: 12px;
  color: var(--text-secondary, #666);
  margin-bottom: 4px;
}

.version .btn {
  margin-top: 10px;
  width: 100%;
}

.vs {
  font-weight: bold;
  color: var(--text-secondary, #888);
}

.btn {
  padding: 8px 14px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
}

.btn.primary {
  background: var(--primary-color, #007aff);
  color: #fff;
}

.btn.secondary {
  background: var(--bg-secondary, #f0f0f0);
  color: var(--text-primary, #333);
}
</style>
