<script setup lang="ts">
import { ref, computed } from 'vue'
import { Moon, Sunny, InfoFilled } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'
import { type ClickEffectType } from '@core/hooks/useClickEffect'

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
  showWatermark, setShowWatermark
} = useSettings()

const clickEffectEnabled = ref(false)
const clickEffectType = ref<ClickEffectType>('explosion')
const clickEffectDuration = ref(400)

const durationMarks = computed(() => ({
  150: t('快'),
  400: t('中'),
  700: t('慢')
}))

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
    size="320px"
    :title="t('偏好设置')"
    direction="ltr"
    @update:model-value="(val: boolean) => emit('update:modelValue', val)"
  >
    <div class="setting-group">
      <p class="setting-label">{{ t('明暗') }}</p>
      <el-radio-group
        :model-value="appearance"
        size="small"
        @change="(val: any) => setAppearance(val)"
      >
        <el-radio-button label="light">
          <el-icon><Sunny /></el-icon>
          {{ t('浅色模式') }}
        </el-radio-button>
        <el-radio-button label="dark">
          <el-icon><Moon /></el-icon>
          {{ t('深色模式') }}
        </el-radio-button>
        <el-radio-button label="system">{{ t('跟随系统') }}</el-radio-button>
      </el-radio-group>
    </div>

    <div class="setting-group">
      <p class="setting-label">{{ t('语言') }}</p>
      <el-radio-group :model-value="locale" size="small" @change="(val: any) => setLocale(val)">
        <el-radio-button label="zh">{{ t('中文') }}</el-radio-button>
        <el-radio-button label="en">{{ t('英文') }}</el-radio-button>
      </el-radio-group>
    </div>

    <div class="setting-group">
      <p class="setting-label">{{ t('界面字体') }}</p>
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
          <el-button size="small" type="primary" plain>{{ t('导入字体') }}</el-button>
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
      <small class="setting-hint">{{ t('支持 TTF/OTF/WOFF 格式') }}</small>
    </div>

    <div class="setting-group">
      <p class="setting-label">{{ t('工具水印') }}</p>
      <el-switch
        :model-value="showWatermark"
        @change="(val: any) => setShowWatermark(val)"
      />
    </div>

    <div class="setting-group">
      <p class="setting-label">{{ t('鼠标点击动画') }}</p>
      <el-switch v-model="clickEffectEnabled" />
      <template v-if="clickEffectEnabled">
        <div class="effect-options">
          <el-radio-group v-model="clickEffectType" size="small">
            <el-radio-button label="explosion">{{ t('粒子爆炸') }}</el-radio-button>
            <el-radio-button label="ripple">{{ t('水波纹') }}</el-radio-button>
            <el-radio-button label="halo">{{ t('光晕') }}</el-radio-button>
          </el-radio-group>
          <div class="duration-slider">
            <span class="duration-label">{{ t('消散时间') }} (150-700ms)</span>
            <el-slider
              v-model="clickEffectDuration"
              :min="150"
              :max="700"
              :step="50"
              :marks="durationMarks"
              :show-tooltip="true"
              :format-tooltip="(val: number) => val + 'ms'"
            />
          </div>
        </div>
      </template>
    </div>

    <div class="setting-group">
      <el-button :icon="InfoFilled" @click="emit('openAbout')">{{ t('关于') }}</el-button>
    </div>
  </el-drawer>
</template>

<style scoped>
.setting-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 14px;
}
.setting-label {
  margin: 0;
  font-weight: 600;
  color: var(--text-primary);
}
.setting-hint {
  color: var(--text-muted);
}
.font-upload-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.effect-options {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 8px;
  padding: 12px;
  background: var(--bg-tertiary);
  border-radius: var(--radius-md);
}
.duration-slider {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.duration-label {
  font-size: 13px;
  color: var(--text-secondary);
}
</style>
