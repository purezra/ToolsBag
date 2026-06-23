<script setup lang="ts">
import { RefreshRight } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'

const props = defineProps<{
  importing: boolean
  batchSize: number
  allowAutoRefresh: boolean
}>()

const emit = defineEmits<{
  (e: 'update:batchSize', value: number): void
  (e: 'update:allowAutoRefresh', value: boolean): void
  (e: 'refresh'): void
}>()

const handleChange = (key: 'batchSize' | 'allowAutoRefresh', value: any) => {
  emit(`update:${key}` as any, value)
}

const { t } = useSettings()
</script>

<template>
  <div class="card-header">
    <div class="header-left">
      <h3 class="header-title">{{ t('视频与图片列表') }}</h3>
    </div>
    <div class="header-right">
      <div class="meta-controls">
        <el-select :model-value="props.batchSize" size="small" style="width: 80px" @change="(val: number) => handleChange('batchSize', val)">
          <el-option v-for="size in [10, 20, 50, 100]" :key="size" :label="`${size}`" :value="size" />
        </el-select>
        <label class="switch-field">
          <span class="switch-label">{{ t('自动') }}</span>
          <el-switch :model-value="props.allowAutoRefresh" size="small" @change="(val: string | number | boolean) => handleChange('allowAutoRefresh', Boolean(val))" />
        </label>
        <el-button :icon="RefreshRight" size="small" @click="emit('refresh')">{{ t('刷新') }}</el-button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px 0;
  flex-shrink: 0;
}
.header-left {
  display: flex;
  align-items: center;
}
.header-title {
  margin: 0;
  font-size: 14px;
  font-weight: 700;
  color: var(--text-primary);
}
.header-right {
  display: flex;
  align-items: center;
  gap: 8px;
}
.meta-controls {
  display: flex;
  align-items: center;
  gap: 6px;
}
.switch-field {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  white-space: nowrap;
}
.switch-label {
  font-size: 11px;
  color: var(--text-muted);
}
</style>
