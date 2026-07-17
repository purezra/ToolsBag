<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue'
import {
  loadWebdavConfig,
  saveWebdavConfig,
  testWebdavConnection,
  getWebdavStatus,
  startWebdavPolling,
  stopWebdavPolling,
  getWebdavChangelog
} from '../api/webdav'
import type { WebdavConfig, WebdavSyncStatus, ChangeLogEntry } from '../types/webdav'

const emit = defineEmits<{
  (e: 'close'): void
}>()

const loading = ref(false)
const testing = ref(false)
const error = ref<string | null>(null)
const success = ref<string | null>(null)
const showPassword = ref(false)
const activeTab = ref<'settings' | 'changelog'>('settings')

const config = ref<WebdavConfig>({
  enabled: false,
  serverUrl: 'https://dav.jianguoyun.com/dav',
  username: '',
  password: '',
  autoSync: true
})

const status = ref<WebdavSyncStatus | null>(null)
const changelog = ref<ChangeLogEntry[]>([])

const isValid = computed(() => {
  return (
    config.value.serverUrl.trim().length > 0 &&
    config.value.username.trim().length > 0 &&
    config.value.password.trim().length > 0
  )
})

const formatTimestamp = (ts: number) => {
  return new Date(ts * 1000).toLocaleString('zh-CN')
}

const actionText = (action: string) => {
  const map: Record<string, string> = {
    added: '新增',
    modified: '修改',
    deleted: '删除'
  }
  return map[action] || action
}

async function loadConfig() {
  loading.value = true
  try {
    const saved = await loadWebdavConfig()
    if (saved) {
      config.value = saved
    }
    status.value = await getWebdavStatus()
  } catch (e: any) {
    error.value = e?.message || '加载配置失败'
  } finally {
    loading.value = false
  }
}

async function loadChangelog() {
  try {
    changelog.value = await getWebdavChangelog()
  } catch (e: any) {
    console.error('加载变更日志失败:', e)
  }
}

async function testConnection() {
  if (!isValid.value) return
  testing.value = true
  error.value = null
  success.value = null
  try {
    await testWebdavConnection(config.value)
    success.value = '连接成功！WebDAV 服务器可访问'
  } catch (e: any) {
    error.value = e?.message || '连接测试失败'
  } finally {
    testing.value = false
  }
}

async function handleSave() {
  loading.value = true
  error.value = null
  success.value = null
  try {
    await saveWebdavConfig(config.value)
    if (config.value.enabled && config.value.autoSync) {
      await startWebdavPolling()
    } else {
      await stopWebdavPolling()
    }
    status.value = await getWebdavStatus()
    success.value = '配置已保存'
  } catch (e: any) {
    error.value = e?.message || '保存失败'
  } finally {
    loading.value = false
  }
}

async function toggleSync() {
  const prev = config.value.enabled
  config.value.enabled = !prev
  try {
    await handleSave()
  } catch (e: any) {
    // 保存失败时回滚开关状态，避免 UI 与后端不一致
    config.value.enabled = prev
    console.error('同步设置保存失败:', e)
  }
}

onMounted(() => {
  loadConfig()
})

watch(activeTab, (tab) => {
  if (tab === 'changelog') {
    loadChangelog()
  }
})
</script>

<template>
  <div class="webdav-settings">
    <div class="header">
      <h3>WebDAV 云同步设置</h3>
      <button class="close-btn" @click="emit('close')">×</button>
    </div>

    <div class="tabs">
      <button
        :class="['tab', { active: activeTab === 'settings' }]"
        @click="activeTab = 'settings'"
      >
        同步设置
      </button>
      <button
        :class="['tab', { active: activeTab === 'changelog' }]"
        @click="activeTab = 'changelog'"
      >
        变更日志
      </button>
    </div>

    <div v-if="activeTab === 'settings'" class="tab-content">
      <div v-if="error" class="alert error">{{ error }}</div>
      <div v-if="success" class="alert success">{{ success }}</div>

      <div class="status-bar" v-if="status">
        <span :class="['status-dot', { connected: status.connected }]"></span>
        <span>{{ status.connected ? '已连接' : '未连接' }}</span>
        <span v-if="status.pollingActive" class="polling-badge">轮询中</span>
        <span v-if="status.remoteVersion" class="version-info">
          远端版本: {{ status.remoteVersion }}
        </span>
      </div>

      <div class="form-group">
        <label>服务器地址</label>
        <input
          v-model="config.serverUrl"
          type="url"
          placeholder="https://dav.jianguoyun.com/dav"
          :disabled="loading"
        />
        <small>坚果云 WebDAV 默认地址已填写</small>
      </div>

      <div class="form-group">
        <label>用户名 / 账号</label>
        <input
          v-model="config.username"
          type="text"
          placeholder="坚果云账号邮箱"
          :disabled="loading"
        />
      </div>

      <div class="form-group">
        <label>应用密码</label>
        <div class="password-input">
          <input
            v-model="config.password"
            :type="showPassword ? 'text' : 'password'"
            placeholder="坚果云应用密码（非登录密码）"
            :disabled="loading"
          />
          <button
            type="button"
            class="toggle-password"
            @click="showPassword = !showPassword"
          >
            {{ showPassword ? '隐藏' : '显示' }}
          </button>
        </div>
        <small>请在坚果云 → 账户信息 → 安全选项 → 第三方应用管理 中生成</small>
      </div>

      <div class="form-group checkbox">
        <label>
          <input type="checkbox" v-model="config.autoSync" :disabled="loading" />
          自动同步（启动时及变更时自动推送）
        </label>
      </div>

      <div class="actions">
        <button
          class="btn secondary"
          @click="testConnection"
          :disabled="!isValid || testing"
        >
          {{ testing ? '测试中...' : '测试连接' }}
        </button>
        <button
          class="btn"
          :class="config.enabled ? 'danger' : 'primary'"
          @click="toggleSync"
          :disabled="!isValid || loading"
        >
          {{ config.enabled ? '停用同步' : '启用同步' }}
        </button>
        <button
          class="btn primary"
          @click="handleSave"
          :disabled="!isValid || loading"
        >
          {{ loading ? '保存中...' : '保存设置' }}
        </button>
      </div>
    </div>

    <div v-if="activeTab === 'changelog'" class="tab-content changelog">
      <div v-if="changelog.length === 0" class="empty">
        暂无变更记录
      </div>
      <div v-else class="log-list">
        <div
          v-for="log in changelog.slice().reverse()"
          :key="`${log.timestamp}-${log.entryId}`"
          class="log-item"
        >
          <span class="time">{{ formatTimestamp(log.timestamp) }}</span>
          <span class="device">{{ log.deviceId.slice(0, 8) }}</span>
          <span :class="['action', log.action]">{{ actionText(log.action) }}</span>
          <span class="title">{{ log.entryTitle }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.webdav-settings {
  background: var(--card-bg, #fff);
  border-radius: 12px;
  padding: 20px;
  max-width: 500px;
  width: 100%;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.header h3 {
  margin: 0;
  font-size: 18px;
}

.close-btn {
  background: none;
  border: none;
  font-size: 24px;
  cursor: pointer;
  color: var(--text-secondary, #666);
}

.tabs {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--border-color, #eee);
  padding-bottom: 8px;
}

.tab {
  padding: 8px 16px;
  border: none;
  background: none;
  cursor: pointer;
  border-radius: 6px;
  color: var(--text-secondary, #666);
}

.tab.active {
  background: var(--primary-color, #007aff);
  color: #fff;
}

.tab-content {
  min-height: 300px;
}

.alert {
  padding: 10px 14px;
  border-radius: 8px;
  margin-bottom: 12px;
  font-size: 14px;
}

.alert.error {
  background: #ffebee;
  color: #c62828;
}

.alert.success {
  background: #e8f5e9;
  color: #2e7d32;
}

.status-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px;
  background: var(--bg-secondary, #f5f5f5);
  border-radius: 8px;
  margin-bottom: 16px;
  font-size: 14px;
}

.status-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #ccc;
}

.status-dot.connected {
  background: #4caf50;
}

.polling-badge {
  background: #2196f3;
  color: #fff;
  padding: 2px 8px;
  border-radius: 10px;
  font-size: 12px;
}

.version-info {
  margin-left: auto;
  color: var(--text-secondary, #666);
}

.form-group {
  margin-bottom: 16px;
}

.form-group label {
  display: block;
  margin-bottom: 6px;
  font-weight: 500;
  font-size: 14px;
}

.form-group input[type="text"],
.form-group input[type="url"],
.form-group input[type="password"] {
  width: 100%;
  padding: 10px 12px;
  border: 1px solid var(--border-color, #ddd);
  border-radius: 8px;
  font-size: 14px;
  box-sizing: border-box;
}

.form-group small {
  display: block;
  margin-top: 4px;
  color: var(--text-secondary, #888);
  font-size: 12px;
}

.form-group.checkbox label {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}

.password-input {
  display: flex;
  gap: 8px;
}

.password-input input {
  flex: 1;
}

.toggle-password {
  padding: 8px 12px;
  border: 1px solid var(--border-color, #ddd);
  background: var(--bg-secondary, #f5f5f5);
  border-radius: 8px;
  cursor: pointer;
  font-size: 12px;
}

.actions {
  display: flex;
  gap: 10px;
  margin-top: 20px;
}

.btn {
  padding: 10px 16px;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
}

.btn.primary {
  background: var(--primary-color, #007aff);
  color: #fff;
}

.btn.secondary {
  background: var(--bg-secondary, #f0f0f0);
  color: var(--text-primary, #333);
}

.btn.danger {
  background: #f44336;
  color: #fff;
}

.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.changelog {
  max-height: 400px;
  overflow-y: auto;
}

.empty {
  text-align: center;
  color: var(--text-secondary, #888);
  padding: 40px 0;
}

.log-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.log-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  background: var(--bg-secondary, #f8f8f8);
  border-radius: 6px;
  font-size: 13px;
}

.log-item .time {
  color: var(--text-secondary, #888);
  font-size: 12px;
}

.log-item .device {
  font-family: monospace;
  color: var(--text-secondary, #666);
}

.log-item .action {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 12px;
}

.log-item .action.added {
  background: #e8f5e9;
  color: #2e7d32;
}

.log-item .action.modified {
  background: #fff3e0;
  color: #ef6c00;
}

.log-item .action.deleted {
  background: #ffebee;
  color: #c62828;
}

.log-item .title {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
