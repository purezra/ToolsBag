<script setup lang="ts">
import { FolderAdd, Upload, Coin } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'

defineProps<{
  importing: boolean
  saving: boolean
  hasItems: boolean
  historyCount: number
  recursive: boolean
}>()

const emit = defineEmits<{
  (e: 'import', kind: 'folder' | 'clipboard'): void
  (e: 'update:recursive', val: boolean): void
  (e: 'addToRename'): void
  (e: 'saveToDb'): void
  (e: 'openHistory'): void
}>()

const { t } = useSettings()
</script>

<template>
  <div class="info-toolbar">
    <div class="toolbar-left">
      <h3 class="toolbar-title">{{ t('视频元数据导出') }}</h3>
      <span class="toolbar-sub">{{ t('导入视频文件，检测 MediaInfo 元数据与潜在质量异常') }}</span>
    </div>
    <div class="toolbar-right">
      <el-button :icon="FolderAdd" round :loading="importing" @click="emit('import', 'folder')">
        {{ t('添加文件夹') }}
      </el-button>
      <el-button :icon="Upload" round plain :loading="importing" @click="emit('import', 'clipboard')">
        {{ t('粘贴路径导入') }}
      </el-button>
      <label class="switch-field">
        <span class="switch-label">{{ t('递归遍历') }}</span>
        <el-switch :model-value="recursive" size="small" @update:model-value="emit('update:recursive', $event)" />
      </label>
      <el-button v-if="hasItems" type="primary" plain round @click="emit('addToRename')">
        {{ t('加入重命名') }}
      </el-button>
      <el-button v-if="hasItems" type="success" :icon="Coin" round :loading="saving" @click="emit('saveToDb')">
        {{ t('入库') }}
      </el-button>
      <el-badge :value="historyCount" :hidden="!historyCount" :max="999">
        <el-button round @click="emit('openHistory')">{{ t('历史记录') }}</el-button>
      </el-badge>
    </div>
  </div>
</template>

<style scoped>
.info-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 14px;
  border-radius: 10px;
  border: 1px solid rgba(20, 23, 31, 0.06);
  background: linear-gradient(145deg, rgba(243, 246, 255, 0.96), rgba(227, 235, 255, 0.9));
  flex-wrap: wrap;
  gap: 8px;
}

.toolbar-left {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.toolbar-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  color: #1a1d26;
}
.toolbar-sub {
  font-size: 12px;
  color: #6d7387;
}

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
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
</style>
