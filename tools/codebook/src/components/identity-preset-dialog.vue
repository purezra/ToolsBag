<script setup lang="ts">
import { ref, computed } from 'vue'
import { Plus, Delete, Download, Upload } from '@element-plus/icons-vue'
import type { CodebookConfig, IdentityCategory } from '@codebook/types/codebook'
import { useSettings } from '@core/hooks/useSettings'

const props = defineProps<{
  visible: boolean
  config: CodebookConfig
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  addPreset: [category: IdentityCategory, value: string]
  removePreset: [category: IdentityCategory, value: string]
  exportPresets: []
  importPresets: [jsonStr: string]
}>()

const { t } = useSettings()

const categories: { key: IdentityCategory; label: string }[] = [
  { key: 'email', label: '邮箱' },
  { key: 'phone', label: '手机号' },
  { key: 'id', label: 'ID/用户名' }
]

const activeCategory = ref<IdentityCategory>('email')
const newValue = ref('')
const fileInput = ref<HTMLInputElement | null>(null)

const currentPresets = computed(() => {
  return props.config.identityPresets
    .filter(p => p.category === activeCategory.value)
    .map(p => p.value)
})

const totalPresets = computed(() => props.config.identityPresets.length)

const handleAdd = () => {
  if (newValue.value.trim()) {
    emit('addPreset', activeCategory.value, newValue.value.trim())
    newValue.value = ''
  }
}

const handleRemove = (value: string) => {
  emit('removePreset', activeCategory.value, value)
}

const handleClose = () => {
  emit('update:visible', false)
}

const handleExport = () => {
  emit('exportPresets')
}

const handleImportClick = () => {
  fileInput.value?.click()
}

const handleFileChange = (e: Event) => {
  const target = e.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) return
  
  const reader = new FileReader()
  reader.onload = (ev) => {
    const content = ev.target?.result as string
    emit('importPresets', content)
  }
  reader.readAsText(file)
  target.value = ''
}
</script>

<template>
  <el-dialog
    :model-value="visible"
    :title="t('预设账号标识')"
    width="520px"
    @update:model-value="(v: boolean) => emit('update:visible', v)"
  >
    <div class="preset-dialog">
      <div class="dialog-toolbar">
        <span class="preset-count">{{ t('共') }} {{ totalPresets }} {{ t('条预设') }}</span>
        <div class="toolbar-actions">
          <el-button size="small" :icon="Download" @click="handleExport">
            {{ t('导出预设') }}
          </el-button>
          <el-button size="small" :icon="Upload" @click="handleImportClick">
            {{ t('导入预设') }}
          </el-button>
          <input
            ref="fileInput"
            type="file"
            accept=".json"
            style="display: none"
            @change="handleFileChange"
          />
        </div>
      </div>

      <el-tabs v-model="activeCategory" type="card">
        <el-tab-pane
          v-for="cat in categories"
          :key="cat.key"
          :label="t(cat.label)"
          :name="cat.key"
        />
      </el-tabs>

      <div class="add-row">
        <el-input
          v-model="newValue"
          :placeholder="t('输入新的') + t(categories.find(c => c.key === activeCategory)?.label || '')"
          @keyup.enter="handleAdd"
        />
        <el-button type="primary" :icon="Plus" @click="handleAdd">
          {{ t('添加') }}
        </el-button>
      </div>

      <div class="preset-list">
        <div v-if="currentPresets.length === 0" class="empty-hint">
          {{ t('暂无预设') }}
        </div>
        <div
          v-for="item in currentPresets"
          :key="item"
          class="preset-item"
        >
          <span class="preset-value">{{ item }}</span>
          <el-button
            type="danger"
            link
            :icon="Delete"
            @click="handleRemove(item)"
          />
        </div>
      </div>
    </div>

    <template #footer>
      <el-button @click="handleClose">{{ t('关闭') }}</el-button>
    </template>
  </el-dialog>
</template>

<style scoped>
.preset-dialog {
  min-height: 200px;
}
.dialog-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}
.preset-count {
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
.toolbar-actions {
  display: flex;
  gap: 8px;
}
.add-row {
  display: flex;
  gap: 8px;
  margin: 16px 0;
}
.preset-list {
  max-height: 240px;
  overflow-y: auto;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 4px;
  padding: 8px;
}
.empty-hint {
  text-align: center;
  color: var(--el-text-color-placeholder);
  padding: 20px;
}
.preset-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  border-radius: 4px;
  transition: background 0.2s;
}
.preset-item:hover {
  background: var(--el-fill-color-light);
}
.preset-value {
  font-family: monospace;
}
</style>
