<script setup lang="ts">
import { FolderAdd, Plus, RefreshRight, Upload } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'

type ImportKind = 'file' | 'folder' | 'clipboard'

const props = defineProps<{
  importing: boolean
  batchSize: number
  allowAutoRefresh: boolean
  recursive: boolean
}>()

const emit = defineEmits<{
  (e: 'update:batchSize', value: number): void
  (e: 'update:allowAutoRefresh', value: boolean): void
  (e: 'update:recursive', value: boolean): void
  (e: 'import', kind: ImportKind): void
  (e: 'refresh'): void
}>()

const handleChange = (key: 'batchSize' | 'allowAutoRefresh' | 'recursive', value: any) => {
  emit(`update:${key}` as any, value)
}

const { t } = useSettings()
</script>

<template>
  <div class="top-actions">
    <div class="left">
      <div class="title">
        <p class="eyebrow">{{ t('批量工具 · V2.0') }}</p>
        <h2>{{ t('视频 / 图片批处理') }}</h2>
        <p class="sub">{{ t('提取视频/图片元数据，批量整理命名，并输出视频体检报告') }}</p>
      </div>
    </div>
    <div class="right">
      <div class="controls">
        <el-button :icon="Plus" round type="primary" :loading="props.importing" @click="emit('import', 'file')">
          {{ t('添加文件') }}
        </el-button>
        <el-button :icon="FolderAdd" round :loading="props.importing" @click="emit('import', 'folder')">
          {{ t('添加文件夹') }}
        </el-button>
        <el-button :icon="Upload" round plain :loading="props.importing" @click="emit('import', 'clipboard')">
          {{ t('粘贴路径导入') }}
        </el-button>
      </div>
      <div class="right-controls">
        <el-select :model-value="props.batchSize" size="small" style="width: 120px" @change="(val: number) => handleChange('batchSize', val)">
          <el-option v-for="size in [10, 20, 50, 100]" :key="size" :label="`${t('批次')} ${size}`" :value="size" />
        </el-select>
        <label class="switch-field">
          <span class="switch-label">{{ t('自动刷新') }}</span>
          <el-switch :model-value="props.allowAutoRefresh" size="small" @change="(val: string | number | boolean) => handleChange('allowAutoRefresh', Boolean(val))" />
        </label>
        <label class="switch-field">
          <span class="switch-label">{{ t('递归遍历') }}</span>
          <el-switch :model-value="props.recursive" size="small" @change="(val: string | number | boolean) => handleChange('recursive', Boolean(val))" />
        </label>
        <el-button :icon="RefreshRight" size="small" @click="emit('refresh')">{{ t('刷新') }}</el-button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.top-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 18px 20px;
  border: 1px solid rgba(20, 23, 31, 0.06);
  border-radius: 16px;
  background: linear-gradient(145deg, rgba(243, 246, 255, 0.96), rgba(227, 235, 255, 0.9));
}
.top-actions .left {
  display: flex;
  align-items: center;
  gap: 14px;
}
.title h2 {
  margin: 0;
}
.title .sub {
  margin: 4px 0 0;
  color: #6d7387;
  font-size: 13px;
}
.controls {
  display: flex;
  gap: 8px;
}
.right {
  display: flex;
  align-items: center;
  gap: 10px;
}
.right-controls {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
}
.switch-field {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
}
.switch-label {
  font-size: 12px;
  color: #4b5570;
}
@media (max-width: 1080px) {
  .top-actions {
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
  }
  .right {
    width: 100%;
    flex-direction: column;
    align-items: flex-start;
  }
  .controls {
    flex-wrap: wrap;
  }
  .right-controls {
    width: 100%;
    justify-content: flex-start;
  }
}
</style>
