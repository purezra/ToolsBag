<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { Moon, Sunny, InfoFilled, Operation, Refresh } from '@element-plus/icons-vue'
import { getVersion } from '@tauri-apps/api/app'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { useSettings } from '@core/hooks/useSettings'
import { formatBytes } from '@core/utils/format'
import { hasTauriRuntime } from '@core/utils/tauri'
import {
  applyUpdate,
  checkUpdate,
  downloadUpdate,
  onUpdateProgress,
  type UpdateInfo
} from '@core/api/update'

defineProps<{
  modelValue: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', val: boolean): void
  (e: 'openAbout'): void
}>()

const {
  t, appearance, setAppearance, locale, setLocale,
  fontFamily, setFontFamily, customFontName, loadCustomFont, clearCustomFont
} = useSettings()

const handleFontUpload = async (file: File) => {
  const success = await loadCustomFont(file)
  if (!success) {
    console.error('Failed to load font')
  }
  return false
}

/* ============ Portable self-update ============ */
const isDesktop = hasTauriRuntime()
const currentVersion = ref('0.2.0')
type UpdateState = 'idle' | 'checking' | 'uptodate' | 'available' | 'downloading' | 'ready' | 'error'
const updateState = ref<UpdateState>('idle')
const updateInfo = ref<UpdateInfo | null>(null)
const updatePath = ref('')
const updateProgress = ref(0)
const updateError = ref('')
let unlistenProgress: UnlistenFn | null = null

const updateStatusText = computed(() => {
  switch (updateState.value) {
    case 'checking': return t('正在检查更新…')
    case 'uptodate': return t('已是最新版本')
    case 'available': return `${t('发现新版本')} v${updateInfo.value?.latest || ''}`
    case 'downloading': return `${t('正在下载…')} ${updateProgress.value}%`
    case 'ready': return t('下载完成，点击重启更新')
    case 'error': return t('更新失败')
    default: return updateInfo.value?.message || (isDesktop ? t('检查是否有新版本') : t('仅桌面端可用'))
  }
})

const checkUpdateAction = async () => {
  if (!isDesktop || updateState.value === 'checking' || updateState.value === 'downloading') return
  updateState.value = 'checking'
  updateError.value = ''
  try {
    const info = await checkUpdate()
    updateInfo.value = info
    updateState.value = info.available ? 'available' : info.message ? 'idle' : 'uptodate'
  } catch (e: any) {
    updateState.value = 'error'
    updateError.value = e?.message || String(e)
  }
}

const downloadUpdateAction = async () => {
  const info = updateInfo.value
  if (!info?.assetUrl) return
  updateState.value = 'downloading'
  updateProgress.value = 0
  updateError.value = ''
  try {
    updatePath.value = await downloadUpdate(info.assetUrl, info.latest, info.sha256)
    updateProgress.value = 100
    updateState.value = 'ready'
  } catch (e: any) {
    updateState.value = 'error'
    updateError.value = e?.message || String(e)
  }
}

const applyUpdateAction = async () => {
  if (!updatePath.value) return
  try {
    await applyUpdate(updatePath.value)
  } catch (e: any) {
    updateState.value = 'error'
    updateError.value = e?.message || String(e)
  }
}

onMounted(async () => {
  if (!isDesktop) return
  try {
    currentVersion.value = await getVersion()
  } catch {
    /* keep package version */
  }
  unlistenProgress = await onUpdateProgress(({ downloaded, total }) => {
    updateProgress.value = total > 0
      ? Math.min(100, Math.round((downloaded / total) * 100))
      : 0
  })
})

onBeforeUnmount(() => {
  unlistenProgress?.()
  unlistenProgress = null
})
</script>

<template>
  <el-drawer
    :model-value="modelValue"
    size="400px"
    :title="t('偏好设置')"
    direction="ltr"
    class="settings-drawer"
    @update:model-value="(val: boolean) => emit('update:modelValue', val)"
  >
    <div class="settings-body">
      <!-- Appearance group -->
      <section class="settings-group">
        <header class="settings-group__head">
          <el-icon :size="15"><Sunny /></el-icon>
          <span>{{ t('界面主题') }}</span>
        </header>

        <div class="settings-row">
          <div class="settings-row__text">
            <p class="settings-row__title">{{ t('明暗') }}</p>
            <p class="settings-row__desc">{{ t('跟随系统将按系统主题切换') }}</p>
          </div>
          <div class="settings-row__control">
            <el-radio-group :model-value="appearance" size="small" @change="(val: any) => setAppearance(val)">
              <el-radio-button label="light"><el-icon><Sunny /></el-icon></el-radio-button>
              <el-radio-button label="dark"><el-icon><Moon /></el-icon></el-radio-button>
              <el-radio-button label="system">{{ t('跟随系统') }}</el-radio-button>
            </el-radio-group>
          </div>
        </div>
      </section>

      <!-- Language & font group -->
      <section class="settings-group">
        <header class="settings-group__head">
          <el-icon :size="15"><Operation /></el-icon>
          <span>{{ t('语言与字体') }}</span>
        </header>

        <div class="settings-row">
          <div class="settings-row__text">
            <p class="settings-row__title">{{ t('语言') }}</p>
          </div>
          <div class="settings-row__control">
            <el-radio-group :model-value="locale" size="small" @change="(val: any) => setLocale(val)">
              <el-radio-button label="zh">{{ t('中文') }}</el-radio-button>
              <el-radio-button label="en">{{ t('英文') }}</el-radio-button>
            </el-radio-group>
          </div>
        </div>

        <div class="settings-row settings-row--stack">
          <div class="settings-row__text">
            <p class="settings-row__title">{{ t('界面字体') }}</p>
            <p class="settings-row__desc">{{ t('支持 TTF/OTF/WOFF 格式') }}</p>
          </div>
          <div class="settings-row__control settings-row__control--full">
            <div class="font-controls">
              <el-radio-group :model-value="fontFamily" size="small" @change="(val: any) => setFontFamily(val)">
                <el-radio-button label="harmonyos">{{ t('默认字体') }}</el-radio-button>
                <el-radio-button label="custom" :disabled="!customFontName">
                  {{ customFontName || t('自定义') }}
                </el-radio-button>
              </el-radio-group>
              <div class="font-upload-row">
                <el-upload
                  :show-file-list="false"
                  accept=".ttf,.otf,.woff,.woff2"
                  :before-upload="handleFontUpload"
                >
                  <el-button size="small" plain>{{ t('导入字体') }}</el-button>
                </el-upload>
                <el-button
                  v-if="customFontName"
                  size="small"
                  type="danger"
                  plain
                  @click="clearCustomFont"
                >
                  {{ t('清除') }}
                </el-button>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- Portable self-update -->
      <section class="settings-group">
        <header class="settings-group__head">
          <el-icon :size="15"><Refresh /></el-icon>
          <span>{{ t('应用更新') }}</span>
        </header>

        <div class="settings-row">
          <div class="settings-row__text">
            <p class="settings-row__title">{{ t('当前版本') }} v{{ currentVersion }}</p>
            <p class="settings-row__desc">{{ updateStatusText }}</p>
          </div>
          <div class="settings-row__control">
            <el-button
              size="small"
              :loading="updateState === 'checking'"
              :disabled="!isDesktop || updateState === 'downloading'"
              @click="checkUpdateAction"
            >
              {{ t('检查更新') }}
            </el-button>
          </div>
        </div>

        <div
          v-if="updateState === 'available' || updateState === 'downloading' || updateState === 'ready'"
          class="update-panel"
        >
          <div class="update-panel__head">
            <span class="update-panel__version">
              {{ t('发现新版本') }} v{{ updateInfo?.latest }}
            </span>
            <span class="update-panel__size">{{ formatBytes(updateInfo?.assetSize || 0) }}</span>
          </div>
          <pre v-if="updateInfo?.notes" class="update-notes">{{ updateInfo.notes }}</pre>
          <el-progress
            v-if="updateState === 'downloading'"
            :percentage="updateProgress"
            :stroke-width="8"
            :show-text="false"
          />
          <div class="update-actions">
            <el-button
              v-if="updateState === 'available'"
              type="primary"
              size="small"
              @click="downloadUpdateAction"
            >
              {{ t('下载更新') }}
            </el-button>
            <el-button v-if="updateState === 'ready'" type="primary" size="small" @click="applyUpdateAction">
              {{ t('重启更新') }}
            </el-button>
          </div>
        </div>

        <p v-if="updateError" class="update-error">{{ updateError }}</p>
      </section>

      <!-- About entry -->
      <section class="settings-group settings-group--footer">
        <button type="button" class="about-entry" @click="emit('openAbout')">
          <el-icon :size="16"><InfoFilled /></el-icon>
          <span class="about-entry__text">
            <span class="about-entry__title">{{ t('关于') }} ToolsBag</span>
            <span class="about-entry__desc">v0.2.0 · Tauri 2 + Vue 3</span>
          </span>
          <el-icon :size="14" class="about-entry__arrow"><InfoFilled /></el-icon>
        </button>
      </section>
    </div>
  </el-drawer>
</template>

<style scoped>
.settings-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  padding: 8px 0 var(--space-5);
}

/* ============ Group (iOS-style grouped list) ============ */
.settings-group {
  display: flex;
  flex-direction: column;
  border: none;
  border-radius: var(--radius-lg);
  background: var(--color-card);
  box-shadow: var(--card-shadow);
  overflow: hidden;
}
.settings-group__head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 14px 16px 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted);
  letter-spacing: 0.01em;
  background: transparent;
  border-bottom: none;
}
.settings-group__head .el-icon {
  color: var(--accent-text);
}

/* ============ Row: title/desc left, control right ============ */
.settings-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  min-height: 56px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-secondary);
}
.settings-row:last-child {
  border-bottom: none;
}
.settings-row--stack {
  flex-direction: column;
  align-items: stretch;
  gap: var(--space-2);
}
.settings-row__text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.settings-row__title {
  margin: 0;
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}
.settings-row__desc {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.4;
}
.settings-row__control {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
.settings-row__control--full {
  width: 100%;
  flex-wrap: wrap;
}

/* Font controls */
.font-controls {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  width: 100%;
}
.font-upload-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

/* ============ About entry ============ */
.settings-group--footer {
  border: none;
  background: transparent;
}
.about-entry {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  width: 100%;
  padding: 14px 16px;
  border: none;
  border-radius: var(--radius-lg);
  background: var(--color-card);
  box-shadow: var(--card-shadow);
  color: var(--text-primary);
  cursor: pointer;
  transition: background var(--duration-fast) var(--ease-standard);
}
.about-entry:hover {
  background: var(--surface-inset);
}
.about-entry__text {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  text-align: left;
}
.about-entry__title {
  font-size: 13px;
  font-weight: 600;
}
.about-entry__desc {
  font-size: 11px;
  color: var(--text-muted);
}
.about-entry__arrow {
  color: var(--text-muted);
}

/* ============ Portable self-update ============ */
.update-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 0 16px 12px;
  padding: 12px;
  border-radius: var(--radius-md);
  background: var(--surface-block);
}
.update-panel__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.update-panel__size {
  flex-shrink: 0;
  font-size: 11px;
  font-weight: 500;
  color: var(--text-muted);
}
.update-notes {
  max-height: 140px;
  margin: 0;
  overflow-y: auto;
  font-family: var(--font-sans);
  font-size: 11px;
  line-height: 1.55;
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-break: break-word;
}
.update-actions {
  display: flex;
  gap: 8px;
}
.update-error {
  margin: 0 16px 12px;
  font-size: 11px;
  line-height: 1.5;
  color: var(--danger-text);
  word-break: break-word;
}

@media (max-width: 480px) {
  .settings-row {
    flex-direction: column;
    align-items: stretch;
  }
  .settings-row__control {
    width: 100%;
    justify-content: flex-start;
  }
}
</style>
