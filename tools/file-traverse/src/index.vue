<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { ElMessage } from 'element-plus'
import { FolderOpened, Upload as UploadIcon, MagicStick, View, ArrowDown, ArrowUp, ArrowRight, Grid, List } from '@element-plus/icons-vue'
import { openParentDir } from '@core/api/common'
import { traverseCopy, checkMediaInfo } from './api/file-traverse'
import { useFileSelect } from '@core/hooks/useFileSelect'
import { useSettings } from '@core/hooks/useSettings'
import { splitPatterns } from './utils/path'
import { formatThroughput, formatBytes as formatFileSize } from '@core/utils/format'
import type { TraverseReq } from './types/file'
import { useTraverse } from './hooks/useTraverse'

const { pick } = useFileSelect()
const { t } = useSettings()

// MediaInfo.dll 检测状态
const mediaInfoAvailable = ref<boolean | null>(null)

const {
  inputDir, outputDir, organize, minSize, maxSize,
  includePatterns, excludePatterns,
  mediaOnlyMode, mediaFiles,
  running, progress, result, problemLog,
  previewing, previewStats, categories, selectedCategories, selectedFormats,
  expandedCategory,
  isAllSelected, isIndeterminate,
  handlePreview, toggleCategory, toggleFormat, toggleCategorySelection,
  selectAllFormatsInCategory, deselectAllFormatsInCategory,
  isCategoryFullySelected, isCategoryPartiallySelected,
  isFormatSelected, getSelectedFormatCount,
  handleSelectAllChange, getEffectiveIncludePatterns, updateSelectState,
  generateOutputDirName,
  // 媒体预览
  mediaPreviewCategories,
  toggleMediaCategory,
  formatFormatsSummary
} = useTraverse()

// 媒体归类模式
const mediaGroupBy = ref<'none' | 'resolution' | 'hdr' | 'codec' | 'format'>('none')

// 获取分组键
const getMediaGroupKey = (file: any, groupBy: string): string => {
  if (groupBy === 'resolution') {
    const w = file.video?.width || file.image?.width
    const h = file.video?.height || file.image?.height
    return w && h ? `${w}×${h}` : '未知'
  }
  if (groupBy === 'hdr') return file.video?.hdrFormat || 'SDR'
  if (groupBy === 'codec') return file.video?.codec || file.audio?.codec || '未知'
  if (groupBy === 'format') return `.${file.ext}`
  return ''
}

// 按指定方式分组文件
const getGroupedFiles = (files: any[], groupBy: string): { key: string; items: any[] }[] => {
  if (groupBy === 'none') return [{ key: '', items: files }]
  const map = new Map<string, any[]>()
  files.forEach(f => {
    const key = getMediaGroupKey(f, groupBy)
    if (!map.has(key)) map.set(key, [])
    map.get(key)!.push(f)
  })
  // 按数量降序
  return Array.from(map.entries())
    .sort((a, b) => b[1].length - a[1].length)
    .map(([key, items]) => ({ key, items }))
}

// Pre-computed filtered media lists to avoid repeated .filter() in template
const videoFiles = computed(() => mediaFiles.value.filter(f => f.mediaType === 'video'))
const audioFiles = computed(() => mediaFiles.value.filter(f => f.mediaType === 'audio'))
const imageFiles = computed(() => mediaFiles.value.filter(f => f.mediaType === 'image'))

// 预览视图模式: card=卡片, table=表格
const previewViewMode = ref<'card' | 'table'>('card')

// 表格视图数据：将 categories 展开为扁平行
const tableData = computed(() => {
  const rows: { category: string; categoryKey: string; ext: string; count: number; selected: boolean }[] = []
  categories.value.forEach(cat => {
    cat.formats.forEach(fmt => {
      rows.push({
        category: cat.label,
        categoryKey: cat.key,
        ext: fmt.ext,
        count: fmt.count,
        selected: isFormatSelected(cat.key, fmt.ext)
      })
    })
  })
  return rows
})

// 表格视图多选变化
const handleTableSelectionChange = (rows: { categoryKey: string; ext: string }[]) => {
  // 先清空所有选择
  categories.value.forEach(cat => {
    selectedFormats.value[cat.key] = []
  })
  selectedCategories.value = []

  // 按分类分组选中的格式
  const grouped: Record<string, string[]> = {}
  rows.forEach(row => {
    const key = row.categoryKey
    if (!grouped[key]) grouped[key] = []
    grouped[key]!.push(row.ext)
  })

  // 更新选择状态
  Object.entries(grouped).forEach(([key, exts]) => {
    selectedFormats.value[key] = exts
    if (!selectedCategories.value.includes(key)) {
      selectedCategories.value.push(key)
    }
  })

  updateSelectState()
}

// Manual raw input refs for pattern input
const includeRaw = ref('')
const excludeRaw = ref('')

// Sync raw input to useTraverse patterns
watch(includeRaw, (val) => {
  includePatterns.value = splitPatterns(val)
})
watch(excludeRaw, (val) => {
  excludePatterns.value = splitPatterns(val)
})

const pickFolder = async (target: 'input' | 'output') => {
  const paths = await pick('folder')
  const picked = paths[0] || ''
  if (!picked) return
  if (target === 'input') inputDir.value = picked
  if (target === 'output') outputDir.value = picked
}

const pastePath = async () => {
  const paths = await pick('clipboard')
  if (paths && paths.length > 0) {
    inputDir.value = paths[0] || ''
  } else {
    ElMessage.warning(t('剪贴板中没有有效路径'))
  }
}

const runTraverse = async () => {
  if (!inputDir.value) {
    ElMessage.warning(t('请选择输入目录'))
    return
  }
  running.value = true
  progress.value = null
  result.value = null
  problemLog.value = ''
  
  const effectiveInclude = getEffectiveIncludePatterns()
  
  let finalInclude = effectiveInclude
  if (!isAllSelected.value && includeRaw.value) {
     finalInclude = [...effectiveInclude, ...includePatterns.value]
  }
  
  // Generate output directory name based on selected formats
  let finalOutputDir = outputDir.value || null
  if (!finalOutputDir) {
    // Get parent directory and base name from inputDir
    const inputPath = inputDir.value.replace(/[\\\/]+$/, '') // Remove trailing slashes
    finalOutputDir = generateOutputDirName(inputPath)
  }
  
  try {
    const res = await traverseCopy({
      inputDir: inputDir.value,
      outputDir: finalOutputDir,
      organizeByFormat: organize.value,
      minSizeMb: minSize.value ?? 0,
      maxSizeMb: maxSize.value ?? null,
      includePatterns: mediaOnlyMode.value ? [] : finalInclude,
      excludePatterns: excludePatterns.value,
      mediaOnlyMode: mediaOnlyMode.value
    } as TraverseReq)
    result.value = res
    
    // 媒体模式：保存媒体文件列表
    if (mediaOnlyMode.value && res.mediaFiles) {
      mediaFiles.value = res.mediaFiles
    }
    
    if (res.problems?.length) {
      problemLog.value = res.problems.map((p) => `${p.path}: ${p.error}`).join('\n')
      ElMessage.warning(`${t('完成但存在问题文件')} (${res.problems.length})`)
    } else {
      problemLog.value = t('暂无问题')
      ElMessage.success(t('提取完成'))
    }
  } catch (e: any) {
    ElMessage.error(e?.toString() || t('执行失败'))
  } finally {
    running.value = false
  }
}

const setupProgress = async () => {
  let unlisten: (() => void) | null = null
  unlisten = await listen('progress-update', (event) => {
    const payload = event.payload as any
    if (!payload || payload.stage !== 'traverse') return
    const percent = Math.floor(((payload.current || 0) / (payload.total || 1)) * 100)
    progress.value = {
      percent,
      message: payload.message || ''
    }
  })
  return unlisten
}

const openOutput = async () => {
  if (!result.value?.outputDir) return
  await openParentDir(result.value.outputDir).catch(() => {
    ElMessage.error(t('无法打开目录'))
  })
}

const openOutputDirPreview = async () => {
  if (!outputDir.value) {
    ElMessage.warning(t('请先设置输出目录'))
    return
  }
  await openParentDir(outputDir.value).catch(() => {
    ElMessage.error(t('无法打开目录'))
  })
}

let unlistenFn: (() => void) | null = null
onMounted(async () => {
  unlistenFn = await setupProgress()
  // 检测 MediaInfo.dll
  try {
    mediaInfoAvailable.value = await checkMediaInfo()
  } catch {
    mediaInfoAvailable.value = false
  }
})
onBeforeUnmount(() => unlistenFn?.())
</script>

<template>
  <div class="file-traverse">
    <!-- 顶部状态栏 -->
    <div class="top-status-bar">
      <div class="mediainfo-status" :class="{ available: mediaInfoAvailable === true, unavailable: mediaInfoAvailable === false }">
        <span class="status-dot"></span>
        <span class="status-text">MediaInfo.dll {{ mediaInfoAvailable === true ? t('已加载') : mediaInfoAvailable === false ? t('未找到') : t('检测中...') }}</span>
      </div>
      <div class="media-mode-toggle">
        <el-switch v-model="mediaOnlyMode" />
        <span class="media-mode-label">{{ t('仅媒体模式') }}</span>
      </div>
    </div>

    <!-- ========== 仅媒体模式 GUI ========== -->
    <template v-if="mediaOnlyMode">
      <div class="form-row">
        <label>{{ t('输入目录') }}</label>
        <div class="inline">
          <el-input v-model="inputDir" :placeholder="t('选择需要遍历的目录')" />
          <el-button :icon="FolderOpened" @click="pickFolder('input')">{{ t('浏览') }}</el-button>
          <el-button :icon="UploadIcon" @click="pastePath">{{ t('粘贴') }}</el-button>
          <el-button :icon="View" type="success" plain :loading="previewing" @click="handlePreview">{{ t('预览') }}</el-button>
        </div>
      </div>

      <!-- 媒体预览折叠块 -->
      <div v-if="mediaPreviewCategories.length > 0" class="media-preview-section">
        <div class="media-preview-header">
          <span>{{ t('发现媒体文件') }}</span>
          <div class="header-right">
            <span class="media-total-count">
              {{ mediaPreviewCategories.reduce((sum, c) => sum + c.files.length, 0) }} {{ t('个文件') }}
            </span>
            <el-select v-model="mediaGroupBy" size="small" style="width: 120px;">
              <el-option :label="t('不归类')" value="none" />
              <el-option :label="t('按分辨率')" value="resolution" />
              <el-option :label="t('按HDR')" value="hdr" />
              <el-option :label="t('按编码')" value="codec" />
              <el-option :label="t('按格式')" value="format" />
            </el-select>
          </div>
        </div>
        
        <div class="media-collapse-list">
          <div v-for="cat in mediaPreviewCategories" :key="cat.type" class="media-collapse-item">
            <div class="media-collapse-header" @click="toggleMediaCategory(cat.type)">
              <div class="collapse-left">
                <el-icon class="collapse-arrow" :class="{ expanded: cat.expanded }">
                  <ArrowRight />
                </el-icon>
                <span class="collapse-icon">{{ cat.icon }}</span>
                <span class="collapse-label">{{ cat.label }}</span>
                <span class="collapse-count">({{ cat.files.length }})</span>
              </div>
              <div class="collapse-formats">
                {{ formatFormatsSummary(cat.formats) }}
              </div>
            </div>
            
            <div v-if="cat.expanded" class="media-collapse-content">
              <template v-for="group in getGroupedFiles(cat.files, mediaGroupBy)" :key="group.key || 'all'">
                <div v-if="mediaGroupBy !== 'none'" class="group-header">
                  <span class="group-label">{{ group.key }}</span>
                  <span class="group-count">{{ group.items.length }} {{ t('个文件') }}</span>
                </div>
                <!-- 视频表格 -->
                <el-table v-if="cat.type === 'video'" :data="group.items" size="small" :max-height="mediaGroupBy === 'none' ? 300 : 200" stripe>
                  <el-table-column type="index" width="50" :label="t('序号')" />
                  <el-table-column prop="name" :label="t('文件名')" min-width="180" show-overflow-tooltip />
                  <el-table-column prop="ext" :label="t('格式')" width="70">
                    <template #default="{ row }">
                      <el-tag size="small" type="info">.{{ row.ext }}</el-tag>
                    </template>
                  </el-table-column>
                  <el-table-column :label="t('分辨率')" width="110" sortable :sort-method="(a: any, b: any) => ((a.video?.width||0)*(a.video?.height||0)) - ((b.video?.width||0)*(b.video?.height||0))">
                    <template #default="{ row }">{{ row.video?.width && row.video?.height ? `${row.video.width}×${row.video.height}` : '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('码率')" width="110" sortable :sort-method="(a: any, b: any) => parseInt(a.video?.bitrate||'0') - parseInt(b.video?.bitrate||'0')">
                    <template #default="{ row }">{{ row.video?.bitrate || '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('时长')" width="90" sortable :sort-method="(a: any, b: any) => (a.video?.durationMs||0) - (b.video?.durationMs||0)">
                    <template #default="{ row }">{{ row.video?.durationStr || '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('帧率')" width="90" sortable :sort-method="(a: any, b: any) => parseFloat(a.video?.frameRate||'0') - parseFloat(b.video?.frameRate||'0')">
                    <template #default="{ row }">{{ row.video?.frameRate || '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('编码')" width="80">
                    <template #default="{ row }">{{ row.video?.codec || '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('HDR')" width="90" sortable :sort-method="(a: any, b: any) => (a.video?.hdrFormat||'').localeCompare(b.video?.hdrFormat||'')">
                    <template #default="{ row }">
                      <el-tag v-if="row.video?.hdrFormat" size="small" type="warning">{{ row.video.hdrFormat }}</el-tag>
                      <span v-else>-</span>
                    </template>
                  </el-table-column>
                  <el-table-column :label="t('大小')" width="90" sortable prop="size">
                    <template #default="{ row }">{{ formatFileSize(row.size) }}</template>
                  </el-table-column>
                </el-table>

                <!-- 音频表格 -->
                <el-table v-else-if="cat.type === 'audio'" :data="group.items" size="small" :max-height="mediaGroupBy === 'none' ? 300 : 200" stripe>
                  <el-table-column type="index" width="50" :label="t('序号')" />
                  <el-table-column prop="name" :label="t('文件名')" min-width="200" show-overflow-tooltip />
                  <el-table-column prop="ext" :label="t('格式')" width="70">
                    <template #default="{ row }">
                      <el-tag size="small" type="info">.{{ row.ext }}</el-tag>
                    </template>
                  </el-table-column>
                  <el-table-column :label="t('编码')" width="100">
                    <template #default="{ row }">{{ row.audio?.codec || '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('采样率')" width="100" sortable :sort-method="(a: any, b: any) => parseInt(a.audio?.sampleRate||'0') - parseInt(b.audio?.sampleRate||'0')">
                    <template #default="{ row }">{{ row.audio?.sampleRate || '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('码率')" width="110" sortable :sort-method="(a: any, b: any) => parseInt(a.audio?.bitrate||'0') - parseInt(b.audio?.bitrate||'0')">
                    <template #default="{ row }">{{ row.audio?.bitrate || '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('时长')" width="90" sortable :sort-method="(a: any, b: any) => (a.audio?.durationMs||0) - (b.audio?.durationMs||0)">
                    <template #default="{ row }">{{ row.audio?.durationStr || '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('大小')" width="90" sortable prop="size">
                    <template #default="{ row }">{{ formatFileSize(row.size) }}</template>
                  </el-table-column>
                </el-table>

                <!-- 图片表格 -->
                <el-table v-else-if="cat.type === 'image'" :data="group.items" size="small" :max-height="mediaGroupBy === 'none' ? 300 : 200" stripe>
                  <el-table-column type="index" width="50" :label="t('序号')" />
                  <el-table-column prop="name" :label="t('文件名')" min-width="220" show-overflow-tooltip />
                  <el-table-column prop="ext" :label="t('格式')" width="70">
                    <template #default="{ row }">
                      <el-tag size="small" type="info">.{{ row.ext }}</el-tag>
                    </template>
                  </el-table-column>
                  <el-table-column :label="t('分辨率')" width="120" sortable :sort-method="(a: any, b: any) => ((a.image?.width||0)*(a.image?.height||0)) - ((b.image?.width||0)*(b.image?.height||0))">
                    <template #default="{ row }">{{ row.image?.width && row.image?.height ? `${row.image.width}×${row.image.height}` : '-' }}</template>
                  </el-table-column>
                  <el-table-column :label="t('大小')" width="100" sortable prop="size">
                    <template #default="{ row }">{{ formatFileSize(row.size) }}</template>
                  </el-table-column>
                </el-table>
              </template>
            </div>
          </div>
        </div>
      </div>

      <!-- 输出目录 -->
      <div class="form-row">
        <label>{{ t('输出目录') }}</label>
        <div class="inline">
          <el-input v-model="outputDir" :placeholder="t('留空则默认：同级目录 + _媒体汇总')" />
          <el-button :icon="FolderOpened" @click="pickFolder('output')">{{ t('浏览') }}</el-button>
        </div>
      </div>

      <!-- 提取按钮 -->
      <div class="media-actions">
        <el-button 
          type="primary" 
          size="large"
          :icon="MagicStick" 
          :loading="running" 
          :disabled="mediaPreviewCategories.length === 0"
          @click="runTraverse"
        >
          {{ t('提取并移动') }}
        </el-button>
        <span class="media-action-hint">{{ t('将按 视频/音频/图片 分类整理到输出目录') }}</span>
      </div>
    </template>

    <!-- ========== 普通模式 GUI ========== -->
    <template v-else>
      <div class="form-row">
        <label>{{ t('输入目录') }}</label>
        <div class="inline">
          <el-input v-model="inputDir" :placeholder="t('选择需要遍历的目录')" />
          <el-button :icon="FolderOpened" @click="pickFolder('input')">{{ t('浏览') }}</el-button>
          <el-button :icon="UploadIcon" @click="pastePath">{{ t('粘贴') }}</el-button>
          <el-button :icon="View" type="success" plain :loading="previewing" @click="handlePreview">{{ t('预览') }}</el-button>
        </div>
      </div>
      
      <!-- Preview / Category Section -->
      <div v-if="categories.length > 0" class="preview-section">
       <div class="category-header">
         <span>{{ t('发现文件类型') }} ({{ previewStats?.totalCount || 0 }})</span>
         <div class="header-right">
           <el-checkbox
             v-if="!mediaOnlyMode"
             :model-value="isAllSelected"
             :indeterminate="isIndeterminate"
             @change="(val: string | number | boolean) => handleSelectAllChange(Boolean(val))"
           >
             {{ t('全选') }}
           </el-checkbox>
           <div class="view-toggle">
             <el-button-group size="small">
               <el-button :type="previewViewMode === 'card' ? 'primary' : ''" :icon="Grid" @click="previewViewMode = 'card'" />
               <el-button :type="previewViewMode === 'table' ? 'primary' : ''" :icon="List" @click="previewViewMode = 'table'" />
             </el-button-group>
           </div>
         </div>
       </div>

       <!-- 卡片视图 -->
       <div v-if="previewViewMode === 'card'" class="category-list-compact">
         <div v-for="cat in categories" :key="cat.key" class="category-chip-group">
           <div
             class="category-chip"
             :class="{
               active: isCategoryFullySelected(cat.key),
               partial: isCategoryPartiallySelected(cat.key),
               expanded: expandedCategory === cat.key
             }"
             @click="toggleCategory(cat.key)"
           >
             <el-checkbox
               :model-value="isCategoryFullySelected(cat.key)"
               :indeterminate="isCategoryPartiallySelected(cat.key)"
               @click.stop
               @change="() => toggleCategorySelection(cat.key)"
             />
             <span class="chip-name">{{ cat.label }}</span>
             <span class="chip-count">{{ cat.count }}</span>
             <el-icon class="chip-arrow">
               <ArrowUp v-if="expandedCategory === cat.key" />
               <ArrowDown v-else />
             </el-icon>
           </div>
           <!-- 展开的格式面板 -->
           <div v-if="expandedCategory === cat.key" class="format-panel">
             <div class="format-header">
               <span>{{ t('选择格式') }} ({{ getSelectedFormatCount(cat.key) }}/{{ cat.formats.length }})</span>
               <div class="format-actions">
                 <el-button size="small" link type="primary" @click="selectAllFormatsInCategory(cat.key)">{{ t('全选') }}</el-button>
                 <el-button size="small" link @click="deselectAllFormatsInCategory(cat.key)">{{ t('清空') }}</el-button>
               </div>
             </div>
             <div class="format-grid">
               <div
                 v-for="fmt in cat.formats"
                 :key="fmt.ext"
                 class="format-item"
                 :class="{ selected: isFormatSelected(cat.key, fmt.ext) }"
                 @click="toggleFormat(cat.key, fmt.ext)"
               >
                 <el-checkbox
                   :model-value="isFormatSelected(cat.key, fmt.ext)"
                   @click.stop
                   @change="() => toggleFormat(cat.key, fmt.ext)"
                 />
                 <span class="format-ext">.{{ fmt.ext }}</span>
                 <span class="format-count">{{ fmt.count }}</span>
               </div>
             </div>
           </div>
         </div>
       </div>

       <!-- 表格视图 -->
       <div v-else class="table-view">
         <el-table :data="tableData" size="small" :max-height="360" stripe border @selection-change="handleTableSelectionChange">
           <el-table-column type="selection" width="45" />
           <el-table-column prop="category" :label="t('分类')" width="90">
             <template #default="{ row }">
               <span class="table-cat-tag">{{ row.category }}</span>
             </template>
           </el-table-column>
           <el-table-column prop="ext" :label="t('格式')" width="100">
             <template #default="{ row }">
               <span class="table-ext">.{{ row.ext }}</span>
             </template>
           </el-table-column>
           <el-table-column prop="count" :label="t('数量')" width="80" sortable />
           <el-table-column :label="t('占比')" min-width="160">
             <template #default="{ row }">
               <div class="ratio-bar-wrap">
                 <div class="ratio-bar" :style="{ width: (row.count / (previewStats?.totalCount || 1) * 100) + '%' }"></div>
                 <span class="ratio-text">{{ (row.count / (previewStats?.totalCount || 1) * 100).toFixed(1) }}%</span>
               </div>
             </template>
           </el-table-column>
         </el-table>
       </div>
    </div>
    
    <div class="form-row">
      <label>{{ t('输出目录') }}</label>
      <div class="inline">
        <el-input v-model="outputDir" :placeholder="t('留空则默认：同级目录 + _汇总')" />
        <el-button :icon="FolderOpened" @click="pickFolder('output')">{{ t('浏览') }}</el-button>
        <el-button type="info" plain @click="openOutputDirPreview">{{ t('打开所在目录') }}</el-button>
      </div>
    </div>

    <div class="grid">
      <div class="form-row">
        <label>{{ t('最小大小(MB)') }}</label>
        <el-input-number v-model="minSize" :min="0" :step="10" />
      </div>
      <div class="form-row">
        <label>{{ t('最大大小(MB)') }}</label>
        <el-input-number v-model="maxSize" :min="0" :step="10" />
      </div>
      <div v-if="!mediaOnlyMode" class="form-row">
        <label>{{ t('相同类型格式自动分组') }}</label>
        <el-switch v-model="organize" />
      </div>
    </div>

    <div v-if="!mediaOnlyMode" class="grid">
      <div class="form-row">
        <label>{{ t('包含模式（逗号分隔）') }}</label>
        <el-input v-model="includeRaw" :placeholder="t('例如：*.jpg,*.png')" />
      </div>
      <div class="form-row">
        <label>{{ t('排除模式（逗号分隔）') }}</label>
        <el-input v-model="excludeRaw" :placeholder="t('例如：*.tmp,*.log')" />
      </div>
    </div>

    <div class="actions">
      <el-button type="primary" :icon="MagicStick" :loading="running" @click="runTraverse">{{ t('开始提取') }}</el-button>
      <span class="hint">{{ t('默认输出：源目录同级的"源名_汇总"') }}</span>
    </div>

    <div v-if="progress" class="progress-card">
      <div class="progress-title">{{ t('进度') }}</div>
      <el-progress :percentage="progress.percent" :text-inside="true" class="flow-progress" />
      <div class="progress-msg">{{ progress.message }}</div>
    </div>

    <div v-if="result" class="result-card">
      <div class="stats">
        <div>{{ t('总文件') }}：{{ result.totalFiles }}</div>
        <div>{{ t('成功') }}：{{ result.copied }} / {{ t('失败') }}：{{ result.failed }} / {{ t('跳过') }}：{{ result.skipped }}</div>
        <div>{{ t('耗时') }}：{{ (result.elapsedMs / 1000).toFixed(2) }} s · {{ t('吞吐') }}：{{ formatThroughput(result.throughputBytes) }}</div>
        <div class="path">{{ t('输出目录') }}：{{ result.outputDir }}</div>
        <el-button text size="small" @click="openOutput">{{ t('打开所在目录') }}</el-button>
      </div>
      <el-table v-if="!mediaOnlyMode" :data="result.formatStats" size="small" height="200" :style="{ width: '100%' }">
        <el-table-column prop="ext" :label="t('格式')" width="80" />
        <el-table-column prop="scanned" :label="t('扫描')" width="80" />
        <el-table-column prop="count" :label="t('导出')" width="80" />
        <el-table-column prop="filtered" :label="t('过滤')" width="80" />
      </el-table>
    </div>

    <!-- 媒体文件详情表格 -->
    <div v-if="mediaOnlyMode && mediaFiles.length > 0" class="media-result">
      <div class="media-result-header">
        <span>{{ t('媒体文件详情') }} ({{ mediaFiles.length }})</span>
      </div>
      
      <!-- 视频列表 -->
      <div v-if="videoFiles.length > 0" class="media-section">
        <div class="media-section-title">🎬 {{ t('视频') }} ({{ videoFiles.length }})</div>
        <el-table :data="videoFiles" size="small" max-height="300">
          <el-table-column prop="newName" :label="t('新文件名')" min-width="150" show-overflow-tooltip />
          <el-table-column :label="t('分辨率')" width="100">
            <template #default="{ row }">{{ row.video?.width }}×{{ row.video?.height }}</template>
          </el-table-column>
          <el-table-column :label="t('码率')" width="120">
            <template #default="{ row }">{{ row.video?.bitrate || '-' }}</template>
          </el-table-column>
          <el-table-column :label="t('时长')" width="80">
            <template #default="{ row }">{{ row.video?.durationStr || '-' }}</template>
          </el-table-column>
          <el-table-column :label="t('帧率')" width="100">
            <template #default="{ row }">{{ row.video?.frameRate }} ({{ row.video?.frameRateMode }})</template>
          </el-table-column>
          <el-table-column :label="t('编码')" width="80">
            <template #default="{ row }">{{ row.video?.codec || '-' }}</template>
          </el-table-column>
          <el-table-column :label="t('宽高比')" width="70">
            <template #default="{ row }">{{ row.video?.aspectRatio || '-' }}</template>
          </el-table-column>
          <el-table-column :label="t('HDR')" width="80">
            <template #default="{ row }">
              <el-tag v-if="row.video?.hdrFormat" size="small" type="warning">{{ row.video.hdrFormat }}</el-tag>
              <span v-else>-</span>
            </template>
          </el-table-column>
        </el-table>
      </div>

      <!-- 音频列表 -->
      <div v-if="audioFiles.length > 0" class="media-section">
        <div class="media-section-title">🎵 {{ t('音频') }} ({{ audioFiles.length }})</div>
        <el-table :data="audioFiles" size="small" max-height="300">
          <el-table-column prop="newName" :label="t('新文件名')" min-width="150" show-overflow-tooltip />
          <el-table-column :label="t('编码')" width="100">
            <template #default="{ row }">{{ row.audio?.codec || '-' }}</template>
          </el-table-column>
          <el-table-column :label="t('采样率')" width="100">
            <template #default="{ row }">{{ row.audio?.sampleRate || '-' }}</template>
          </el-table-column>
          <el-table-column :label="t('码率')" width="120">
            <template #default="{ row }">{{ row.audio?.bitrate || '-' }}</template>
          </el-table-column>
          <el-table-column :label="t('时长')" width="100">
            <template #default="{ row }">{{ row.audio?.durationStr || '-' }}</template>
          </el-table-column>
          <el-table-column :label="t('大小')" width="100">
            <template #default="{ row }">{{ formatFileSize(row.size) }}</template>
          </el-table-column>
        </el-table>
      </div>

      <!-- 图片列表 -->
      <div v-if="imageFiles.length > 0" class="media-section">
        <div class="media-section-title">🖼️ {{ t('图片') }} ({{ imageFiles.length }})</div>
        <el-table :data="imageFiles" size="small" max-height="300">
          <el-table-column prop="newName" :label="t('新文件名')" min-width="150" show-overflow-tooltip />
          <el-table-column :label="t('格式')" width="80">
            <template #default="{ row }">{{ row.image?.format || '-' }}</template>
          </el-table-column>
          <el-table-column :label="t('分辨率')" width="120">
            <template #default="{ row }">{{ row.image?.width }}×{{ row.image?.height }}</template>
          </el-table-column>
          <el-table-column :label="t('大小')" width="100">
            <template #default="{ row }">{{ formatFileSize(row.size) }}</template>
          </el-table-column>
        </el-table>
      </div>
    </div>

    <div class="log">
      <div class="log-title">{{ t('问题日志') }}</div>
      <pre class="log-box">{{ problemLog || t('暂无问题') }}</pre>
    </div>
    </template>
  </div>
</template>

<style scoped>
.file-traverse {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
/* 顶部状态栏 */
.top-status-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  background: #f5f7fa;
  border-radius: 8px;
  margin-bottom: 4px;
}
.mediainfo-status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: #909399;
}
.mediainfo-status .status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #909399;
}
.mediainfo-status.available .status-dot {
  background: #67c23a;
}
.mediainfo-status.available .status-text {
  color: #67c23a;
}
.mediainfo-status.unavailable .status-dot {
  background: #f56c6c;
}
.mediainfo-status.unavailable .status-text {
  color: #f56c6c;
}
.media-mode-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  background: linear-gradient(135deg, #fff7e6, #fff3cd);
  border: 1px solid #ffc107;
  border-radius: 20px;
  padding: 4px 12px;
}
.media-mode-label {
  font-size: 13px;
  color: #856404;
  font-weight: 500;
}
.form-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.inline {
  display: flex;
  gap: 8px;
}
.grid {
  display: grid;
  gap: 10px;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
}
.actions {
  display: flex;
  align-items: center;
  gap: 12px;
}
.hint {
  color: #7a8194;
  font-size: 12px;
}
.progress-card {
  border: 1px solid rgba(20, 23, 31, 0.08);
  padding: 10px;
  border-radius: 10px;
  background: #f8fbff;
}
.progress-title {
  font-weight: 600;
  margin-bottom: 4px;
}
.progress-msg {
  margin-top: 4px;
  color: #666;
}
.flow-progress .el-progress-bar__inner {
  background: linear-gradient(120deg, #6dd5ed, #2193b0, #6dd5ed);
  background-size: 200% 200%;
  animation: flow-bar 1.2s linear infinite;
}
@keyframes flow-bar {
  0% {
    background-position: 0% 50%;
  }
  100% {
    background-position: 200% 50%;
  }
}
.result-card {
  border: 1px solid #e5e5e5;
  border-radius: 10px;
  padding: 12px;
  background: #fafafa;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.stats {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 13px;
}
.path {
  font-family: Consolas, 'SFMono-Regular', monospace;
  word-break: break-all;
}
.log {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.log-box {
  border: 1px solid #e5e5e5;
  background: #f8f8f8;
  padding: 8px;
  min-height: 120px;
  border-radius: 6px;
  white-space: pre-wrap;
}

/* Preview Section */
.preview-section {
  background: #f0f7ff;
  border: 1px dashed #a0cfff;
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 8px;
}
.category-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
  font-weight: 600;
  color: #409eff;
}
.header-right {
  display: flex;
  align-items: center;
  gap: 12px;
}
.view-toggle .el-button-group .el-button {
  padding: 5px 8px;
}

/* 卡片视图 - 紧凑水平排列 */
.category-list-compact {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: flex-start;
}
.category-chip-group {
  display: flex;
  flex-direction: column;
}
.category-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: #fff;
  border: 1px solid #dcdfe6;
  border-radius: 16px;
  padding: 4px 10px;
  cursor: pointer;
  transition: border-color 0.2s, background 0.2s;
  user-select: none;
  font-size: 13px;
}
.category-chip:hover {
  border-color: #409eff;
}
.category-chip.active {
  background: #409eff;
  color: #fff;
  border-color: #409eff;
}
.category-chip.active .chip-count {
  background: rgba(255,255,255,0.25);
  color: #fff;
}
.category-chip.partial {
  background: #ecf5ff;
  border-color: #409eff;
  color: #409eff;
}
.category-chip.expanded {
  border-color: #409eff;
}
.chip-name {
  font-weight: 500;
}
.chip-count {
  font-size: 11px;
  background: rgba(0,0,0,0.08);
  padding: 0 5px;
  border-radius: 8px;
  color: #606266;
}
.chip-arrow {
  font-size: 11px;
  color: #909399;
}
.category-chip.active .chip-arrow {
  color: rgba(255,255,255,0.7);
}

/* Format Panel - 改为相对定位，避免重叠 */
.format-panel {
  margin-top: 8px;
  margin-left: 12px;
  background: #fff;
  border: 1px solid #e4e7ed;
  border-radius: 8px;
  padding: 12px;
  box-shadow: 0 2px 8px rgba(0,0,0,0.06);
}
.format-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
  font-size: 13px;
  color: #606266;
}
.format-actions {
  display: flex;
  gap: 8px;
}
.format-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(90px, 1fr));
  gap: 6px;
  max-height: 200px;
  overflow-y: auto;
}
.format-item {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  border: 1px solid #e4e7ed;
  border-radius: 4px;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
  font-size: 12px;
}
.format-item:hover {
  border-color: #409eff;
  background: #f0f7ff;
}
.format-item.selected {
  background: #ecf5ff;
  border-color: #409eff;
}
.format-ext {
  font-family: 'JetBrains Mono', Consolas, monospace;
  font-weight: 500;
}
.format-count {
  margin-left: auto;
  color: #909399;
  font-size: 11px;
}

/* 表格视图 */
.table-view {
  background: #fff;
  border-radius: 6px;
  overflow: hidden;
}
.table-cat-tag {
  font-size: 12px;
  font-weight: 500;
  color: #409eff;
}
.table-ext {
  font-family: 'JetBrains Mono', Consolas, monospace;
  font-weight: 500;
  font-size: 13px;
}
.ratio-bar-wrap {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 16px;
}
.ratio-bar {
  height: 6px;
  background: linear-gradient(90deg, #409eff, #79bbff);
  border-radius: 3px;
  min-width: 2px;
  transition: width 0.3s;
}
.ratio-text {
  font-size: 11px;
  color: #909399;
  white-space: nowrap;
}

/* 媒体结果 */
.media-result {
  border: 1px solid #e5e5e5;
  border-radius: 10px;
  padding: 12px;
  background: #fafafa;
}
.media-result-header {
  font-weight: 600;
  margin-bottom: 12px;
  color: #333;
}
.media-section {
  margin-bottom: 16px;
}
.media-section:last-child {
  margin-bottom: 0;
}
.media-section-title {
  font-weight: 500;
  margin-bottom: 8px;
  color: #606266;
  font-size: 14px;
}

/* 媒体预览折叠块 */
.media-preview-section {
  background: linear-gradient(135deg, #f0f7ff, #e8f4fd);
  border: 1px solid #b3d8ff;
  border-radius: 10px;
  padding: 16px;
  margin: 8px 0;
}
.media-preview-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
  font-weight: 600;
  color: #409eff;
}
.media-total-count {
  font-size: 13px;
  color: #909399;
  font-weight: normal;
}
.media-collapse-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.media-collapse-item {
  background: #fff;
  border-radius: 8px;
  overflow: hidden;
  box-shadow: 0 1px 4px rgba(0,0,0,0.06);
}
.media-collapse-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  cursor: pointer;
  transition: background 0.2s;
}
.media-collapse-header:hover {
  background: #f5f7fa;
}
.collapse-left {
  display: flex;
  align-items: center;
  gap: 8px;
}
.collapse-arrow {
  transition: transform 0.2s;
  color: #909399;
}
.collapse-arrow.expanded {
  transform: rotate(90deg);
}
.collapse-icon {
  font-size: 18px;
}
.collapse-label {
  font-weight: 500;
  color: #303133;
}
.collapse-count {
  color: #909399;
  font-size: 13px;
}
.collapse-formats {
  font-size: 12px;
  color: #606266;
  max-width: 60%;
  text-align: right;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.media-collapse-content {
  border-top: 1px solid #ebeef5;
  padding: 12px;
  background: #fafafa;
}
.group-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 0;
  margin-bottom: 4px;
}
.group-label {
  font-size: 13px;
  font-weight: 600;
  color: #303133;
  background: #e8f4fd;
  padding: 2px 10px;
  border-radius: 10px;
}
.group-count {
  font-size: 12px;
  color: #909399;
}

/* 媒体操作按钮 */
.media-actions {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px dashed #dcdfe6;
}
.media-action-hint {
  font-size: 12px;
  color: #909399;
}
</style>
