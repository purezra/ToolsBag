<script setup lang="ts">
import { computed } from 'vue'
import { FolderAdd, MoreFilled, Plus, RefreshRight, Upload } from '@element-plus/icons-vue'
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

const { t, theme } = useSettings()
const isApple = computed(() => theme.value === 'apple')
</script>

<template>
  <div class="top-actions" :class="{ 'is-apple': isApple }">
    <div class="left">
      <div class="icon" v-if="isApple"></div>
      <div class="title">
        <p class="eyebrow" v-if="!isApple">{{ t('批量工具 · V2.0') }}</p>
        <h2>{{ t('视频 / 图片批处理') }}</h2>
        <p class="sub">{{ t('批量提取视频/图片信息，重命名与可视化分析，一站式处理') }}</p>
      </div>
    </div>
    <div class="right" v-if="isApple">
      <el-button :icon="Plus" type="primary" round :loading="props.importing" @click="emit('import', 'file')">
        {{ t('添加文件') }}
      </el-button>
      <el-button type="primary" round plain :icon="RefreshRight" @click="emit('refresh')">
        {{ t('开始提取') }}
      </el-button>
      <el-dropdown>
        <el-button circle plain>
          <el-icon><MoreFilled /></el-icon>
        </el-button>
        <template #dropdown>
          <el-dropdown-menu>
            <el-dropdown-item @click="emit('import', 'folder')">
              <el-icon><FolderAdd /></el-icon>
              <span>{{ t('添加文件夹') }}</span>
            </el-dropdown-item>
            <el-dropdown-item @click="emit('import', 'clipboard')">
              <el-icon><Upload /></el-icon>
              <span>{{ t('粘贴路径导入') }}</span>
            </el-dropdown-item>
          </el-dropdown-menu>
        </template>
      </el-dropdown>
    </div>
    <div class="right" v-else>
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
  <div class="right-controls apple-controls" v-if="isApple">
    <el-select :model-value="props.batchSize" size="small" class="apple-select" @change="(val: number) => handleChange('batchSize', val)">
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
.apple-controls {
  margin-top: 8px;
  justify-content: flex-end;
}
.apple-select {
  min-width: 160px;
}
.top-actions.is-apple {
  border: none;
  background: rgba(255, 255, 255, 0.85);
  backdrop-filter: blur(15px);
  border-radius: 18px;
  box-shadow: 0 14px 36px rgba(0, 0, 0, 0.08);
  position: relative;
  top: auto;
  z-index: auto;
}
.top-actions.is-apple .icon {
  width: 32px;
  height: 32px;
  border-radius: 10px;
  background: linear-gradient(135deg, #0071e3, #51a6ff);
  box-shadow: 0 10px 24px rgba(0, 113, 227, 0.28);
}
.top-actions.is-apple .title .sub,
.top-actions.is-apple .eyebrow {
  color: #6e6e73;
}
.top-actions.is-apple .controls {
  display: none;
}
.top-actions.is-apple .right {
  gap: 12px;
}
.top-actions.is-apple .title h2 {
  font-size: 20px;
  font-weight: 600;
  letter-spacing: 0.5px;
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
