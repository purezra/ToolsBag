<script setup lang="ts">
import { computed } from 'vue'
import { Moon, Sunny, InfoFilled, MagicStick, Operation } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'
import type { WatermarkMode } from '@core/hooks/useSettings'

defineProps<{
  modelValue: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', val: boolean): void
  (e: 'openAbout'): void
}>()

const {
  t, appearance, setAppearance, locale, setLocale,
  fontFamily, setFontFamily, customFontName, loadCustomFont, clearCustomFont,
  showWatermark, setShowWatermark, watermarkMode, setWatermarkMode,
  watermarkOpacity, setWatermarkOpacity, watermarkScale, setWatermarkScale
} = useSettings()

const watermarkOpacityMarks = computed(() => ({
  2: '2%',
  12: '12%',
  30: '30%'
}))

const watermarkScaleMarks = computed(() => ({
  40: '小',
  100: '中',
  200: '大'
}))

const watermarkModeOptions: { value: WatermarkMode; labelKey: string }[] = [
  { value: 'static', labelKey: '水印·静态' },
  { value: 'tile', labelKey: '水印·平铺' },
  { value: 'float', labelKey: '水印·漂浮' },
  { value: 'breath', labelKey: '水印·呼吸' }
]

const handleFontUpload = async (file: File) => {
  const success = await loadCustomFont(file)
  if (!success) {
    console.error('Failed to load font')
  }
  return false
}
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

      <!-- Effects group -->
      <section class="settings-group">
        <header class="settings-group__head">
          <el-icon :size="15"><MagicStick /></el-icon>
          <span>{{ t('视觉效果') }}</span>
        </header>

        <div class="settings-row settings-row--stack">
          <div class="settings-row__text">
            <p class="settings-row__title">{{ t('工具水印') }}</p>
            <p class="settings-row__desc">{{ t('在工具页面叠加图标水印，可调模式与透明度') }}</p>
          </div>
          <div class="settings-row__control settings-row__control--full">
            <div class="watermark-controls">
              <el-switch :model-value="showWatermark" @change="(val: any) => setShowWatermark(val)" />
              <template v-if="showWatermark">
                <div class="watermark-options">
                  <el-radio-group :model-value="watermarkMode" size="small" @change="(val: any) => setWatermarkMode(val)">
                    <el-radio-button
                      v-for="opt in watermarkModeOptions"
                      :key="opt.value"
                      :label="opt.value"
                    >{{ t(opt.labelKey) }}</el-radio-button>
                  </el-radio-group>
                  <div class="watermark-slider">
                    <span class="watermark-slider__label">{{ t('透明度') }}</span>
                    <el-slider
                      :model-value="watermarkOpacity"
                      :min="2"
                      :max="30"
                      :step="1"
                      :marks="watermarkOpacityMarks"
                      :show-tooltip="true"
                      :format-tooltip="(val: number) => val + '%'"
                      @change="(val: any) => setWatermarkOpacity(Number(val))"
                    />
                  </div>
                  <div class="watermark-slider">
                    <span class="watermark-slider__label">{{ t('大小') }}</span>
                    <el-slider
                      :model-value="watermarkScale"
                      :min="40"
                      :max="200"
                      :step="10"
                      :marks="watermarkScaleMarks"
                      :show-tooltip="true"
                      :format-tooltip="(val: number) => val + '%'"
                      @change="(val: any) => setWatermarkScale(Number(val))"
                    />
                  </div>
                </div>
              </template>
            </div>
          </div>
        </div>
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
  gap: var(--space-4);
  padding: 4px 0 var(--space-5);
}

/* ============ Group ============ */
.settings-group {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--card-border);
  border-radius: var(--radius-card);
  background: var(--color-card);
  overflow: hidden;
}
.settings-group__head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px var(--space-4);
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  letter-spacing: 0.02em;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-secondary);
}
.settings-group__head .el-icon {
  color: var(--accent);
}

/* ============ Row: title/desc left, control right ============ */
.settings-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
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
  font-size: 13px;
  font-weight: 600;
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

/* Watermark controls */
.watermark-controls {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: 100%;
}
.watermark-options {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: 100%;
  padding: var(--space-3);
  background: var(--bg-secondary);
  border: 1px solid var(--border-secondary);
  border-radius: var(--radius-md);
}
.watermark-slider {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.watermark-slider__label {
  font-size: 12px;
  color: var(--text-secondary);
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
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--card-border);
  border-radius: var(--radius-md);
  background: var(--color-card);
  color: var(--text-primary);
  cursor: pointer;
  transition: border-color var(--duration-fast) ease, background var(--duration-fast) ease;
}
.about-entry:hover {
  border-color: var(--accent);
  background: var(--accent-light);
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
