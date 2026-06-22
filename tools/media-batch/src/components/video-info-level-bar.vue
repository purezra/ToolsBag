<script setup lang="ts">
import { Grid, List, Download, Filter } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'
import type { DisplayLevel } from '../types/media'

export type GroupByOption = 'none' | 'resolution' | 'resolutionTier' | 'orientation' | 'codec' | 'hdr' | 'bitDepth' | 'format' | 'frameRate'
export type ExportFormat = 'xlsx' | 'csv' | 'markdown' | 'html' | 'json' | 'txt' | 'xml2json' | 'xml2md'
export type EmptyColMode = 'right' | 'hide'

defineProps<{
  hasItems: boolean
  level: DisplayLevel
  levelOptions: { label: string; value: DisplayLevel }[]
  levelDesc: string
  healthSummary: { danger: number; warning: number; info: number; totalIssues: number }
  viewMode: 'card' | 'table'
  emptyColMode: EmptyColMode
  highlightDiff: boolean
  groupBy: GroupByOption
  groupOptions: { label: string; value: GroupByOption }[]
  searchQuery: string
}>()

const emit = defineEmits<{
  (e: 'update:level', val: DisplayLevel): void
  (e: 'update:viewMode', val: 'card' | 'table'): void
  (e: 'update:emptyColMode', val: EmptyColMode): void
  (e: 'update:highlightDiff', val: boolean): void
  (e: 'update:groupBy', val: GroupByOption): void
  (e: 'update:searchQuery', val: string): void
  (e: 'expandAll'): void
  (e: 'collapseAll'): void
  (e: 'export', format: ExportFormat): void
  (e: 'clear'): void
}>()

const { t } = useSettings()
</script>

<template>
  <div v-if="hasItems" class="level-bar">
    <div class="level-selector">
      <span class="level-label">{{ t('显示等级') }}</span>
      <el-segmented :model-value="level" :options="levelOptions" size="small" @update:model-value="emit('update:level', $event)" />
    </div>
    <div class="level-desc">{{ t(levelDesc) }}</div>
    <div class="level-actions">
      <el-tag v-if="healthSummary.danger" size="small" type="danger">{{ t('严重') }} {{ healthSummary.danger }}</el-tag>
      <el-tag v-if="healthSummary.warning" size="small" type="warning">{{ t('警告') }} {{ healthSummary.warning }}</el-tag>
      <el-tag v-if="healthSummary.info" size="small" type="info">{{ t('提示') }} {{ healthSummary.info }}</el-tag>
      <el-tag v-if="!healthSummary.totalIssues" size="small" type="success">{{ t('未发现异常') }}</el-tag>
      <el-select v-if="viewMode === 'table'" :model-value="emptyColMode" size="small" style="width: 130px;" @update:model-value="emit('update:emptyColMode', $event)">
        <el-option :label="t('全空列靠右')" value="right" />
        <el-option :label="t('全空列隐藏')" value="hide" />
      </el-select>
      <label v-if="viewMode === 'table'" class="switch-field">
        <span class="switch-label">{{ t('高亮差异') }}</span>
        <el-switch :model-value="highlightDiff" size="small" @update:model-value="emit('update:highlightDiff', $event)" />
      </label>
      <el-select v-if="viewMode === 'table'" :model-value="groupBy" size="small" style="width: 120px;" @update:model-value="emit('update:groupBy', $event)">
        <el-option v-for="opt in groupOptions" :key="opt.value" :label="opt.label" :value="opt.value" />
      </el-select>
      <el-input
        :model-value="searchQuery"
        size="small"
        :placeholder="t('搜索文件名...')"
        clearable
        style="width: 200px"
        @update:model-value="emit('update:searchQuery', $event)"
      />
      <el-button-group size="small">
        <el-button :type="viewMode === 'card' ? 'primary' : ''" :icon="Grid" @click="emit('update:viewMode', 'card')" />
        <el-button :type="viewMode === 'table' ? 'primary' : ''" :icon="List" @click="emit('update:viewMode', 'table')" />
      </el-button-group>
      <el-button v-if="viewMode === 'card'" size="small" type="primary" plain @click="emit('expandAll')">{{ t('全部展开') }}</el-button>
      <el-button v-if="viewMode === 'card'" size="small" type="primary" plain @click="emit('collapseAll')">{{ t('全部收起') }}</el-button>
      <el-dropdown v-if="viewMode === 'table'" trigger="click" @command="(cmd: any) => emit('export', cmd as ExportFormat)">
        <el-button size="small" type="primary" plain :icon="Download">{{ t('导出') }}</el-button>
        <template #dropdown>
          <el-dropdown-menu>
            <el-dropdown-item command="xlsx">XLSX</el-dropdown-item>
            <el-dropdown-item command="csv">CSV</el-dropdown-item>
            <el-dropdown-item command="markdown">Markdown {{ t('报告') }}</el-dropdown-item>
            <el-dropdown-item command="html">HTML {{ t('报告') }}</el-dropdown-item>
            <el-dropdown-item command="json">JSON</el-dropdown-item>
            <el-dropdown-item divided command="txt">TXT {{ t('完整') }}</el-dropdown-item>
            <el-dropdown-item command="xml2json">XML→JSON</el-dropdown-item>
            <el-dropdown-item command="xml2md">XML→MD</el-dropdown-item>
          </el-dropdown-menu>
        </template>
      </el-dropdown>
      <el-button size="small" type="danger" plain @click="emit('clear')">{{ t('清空') }}</el-button>
    </div>
  </div>
</template>

<style scoped>
.level-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 8px;
  border: 1px solid rgba(20, 23, 31, 0.06);
  background: #fff;
  flex-wrap: wrap;
}

.level-selector {
  display: flex;
  align-items: center;
  gap: 10px;
}
.level-label {
  font-size: 13px;
  font-weight: 600;
  color: #303133;
  white-space: nowrap;
}
.level-desc {
  font-size: 12px;
  color: #8b8fa3;
  flex: 1;
}
.level-actions {
  display: flex;
  align-items: center;
  gap: 4px;
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
