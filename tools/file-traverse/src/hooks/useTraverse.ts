import { ref, computed } from 'vue'
import { readDir } from '@tauri-apps/plugin-fs'
import { ElMessage } from 'element-plus'
import type { TraverseResp, PreviewStats, MediaFileInfo, MediaPreviewFile } from '../types/file'
import { previewMedia } from '../api/file-traverse'

export interface FormatInfo {
  ext: string
  count: number
}

export interface CategoryInfo {
  key: string
  label: string
  count: number
  formats: FormatInfo[]
}

/** 媒体预览文件 */
export interface MediaPreviewFileLocal {
  name: string
  path: string
  ext: string
  size: number
}

/** 媒体预览分类 */
export interface MediaPreviewCategory {
  type: 'video' | 'audio' | 'image'
  label: string
  icon: string
  files: MediaPreviewFile[]
  formats: Record<string, number>  // ext -> count
  expanded: boolean
}

export function useTraverse() {
  const inputDir = ref('')
  const outputDir = ref('')
  const organize = ref(true)
  const minSize = ref<number | null>(null)
  const maxSize = ref<number | null>(null)
  const includePatterns = ref<string[]>([])
  const excludePatterns = ref<string[]>([])
  
  // 仅媒体模式
  const mediaOnlyMode = ref(false)

  const running = ref(false)
  const progress = ref<{ percent: number; message: string } | null>(null)
  const result = ref<TraverseResp | null>(null)
  const problemLog = ref('')
  
  // 媒体文件结果
  const mediaFiles = ref<MediaFileInfo[]>([])
  
  // 媒体预览数据
  const mediaPreviewCategories = ref<MediaPreviewCategory[]>([])
  
  // Preview state
  const previewing = ref(false)
  const previewStats = ref<PreviewStats | null>(null)
  const selectedCategories = ref<string[]>([])
  const selectedFormats = ref<Record<string, string[]>>({}) // { 'image': ['jpg', 'png'], ... }
  const expandedCategory = ref<string | null>(null)
  const isAllSelected = ref(true)
  const isIndeterminate = ref(false)

  // Categories mapping based on extensions
  const categoryMap: Record<string, string[]> = {
    'image': ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'svg', 'tiff', 'tif', 'ico', 'heic', 'heif', 'avif', 'raw', 'psd'],
    'video': ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v', '3gp', 'ts', 'mts', 'm2ts'],
    'audio': ['mp3', 'wav', 'flac', 'aac', 'ogg', 'm4a', 'wma', 'ape', 'aiff', 'opus', 'dts', 'ac3'],
    'document': ['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'md', 'rtf', 'csv', 'epub', 'mobi'],
    'archive': ['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'iso', 'dmg'],
    'code': ['js', 'ts', 'jsx', 'tsx', 'html', 'css', 'scss', 'less', 'json', 'xml', 'yaml', 'yml', 'py', 'java', 'c', 'cpp', 'h', 'hpp', 'rs', 'go', 'php', 'rb', 'swift', 'kt', 'vue', 'svelte']
  }

  // 媒体模式下的分类标签（中文）
  const mediaCategoryLabels: Record<string, string> = {
    'video': '视频',
    'audio': '音频',
    'image': '图片'
  }

  // 媒体模式下只显示这三个分类
  const mediaCategories = ['video', 'audio', 'image']

  const categories = computed<CategoryInfo[]>(() => {
    if (!previewStats.value) return []
    
    const cats: Record<string, { label: string; count: number; formats: Record<string, number> }> = {}
    
    previewStats.value.files.forEach(file => {
      const ext = file.name.split('.').pop()?.toLowerCase() || 'unknown'
      let category = 'other'
      
      for (const [cat, exts] of Object.entries(categoryMap)) {
        if (exts.includes(ext)) {
          category = cat
          break
        }
      }
      
      // 媒体模式下只统计媒体类型
      if (mediaOnlyMode.value && !mediaCategories.includes(category)) {
        return
      }
      
      if (!cats[category]) {
        cats[category] = { label: category, count: 0, formats: {} }
      }
      const cat = cats[category]!
      cat.count++
      cat.formats[ext] = (cat.formats[ext] || 0) + 1
    })
    
    // 媒体模式下按固定顺序排列：视频、音频、图片
    if (mediaOnlyMode.value) {
      const sorted = mediaCategories
        .filter(key => cats[key])
        .map(key => ({
          key,
          label: mediaCategoryLabels[key] || key,
          count: cats[key]!.count,
          formats: Object.entries(cats[key]!.formats)
            .sort((a, b) => b[1] - a[1])
            .map(([ext, count]) => ({ ext, count }))
        }))
      return sorted
    }
    
    // 普通模式：按数量排序
    const sorted = Object.entries(cats)
      .sort((a, b) => b[1].count - a[1].count)
      .map(([key, val]) => ({
        key,
        label: key.charAt(0).toUpperCase() + key.slice(1),
        count: val.count,
        formats: Object.entries(val.formats)
          .sort((a, b) => b[1] - a[1])
          .map(([ext, count]) => ({ ext, count }))
      }))
    
    return sorted
  })

  const handlePreview = async () => {
    if (!inputDir.value) {
      ElMessage.warning('请先选择输入目录')
      return
    }
    
    previewing.value = true
    expandedCategory.value = null
    mediaPreviewCategories.value = []
    
    try {
      // 媒体模式：调用后端获取元数据
      if (mediaOnlyMode.value) {
        const result = await previewMedia({ inputDir: inputDir.value })
        buildMediaPreviewCategoriesFromBackend(result.videos, result.audios, result.images)
      } else {
        // 普通模式：只扫描文件列表
        const files = await flattenEntries(inputDir.value)
        previewStats.value = {
          files,
          totalCount: files.length,
          totalSize: files.reduce((sum, f) => sum + f.size, 0)
        }
        selectAllCategories()
      }
    } catch (e) {
      console.error(e)
      ElMessage.error('预览失败')
    } finally {
      previewing.value = false
    }
  }

  // 从后端结果构建媒体预览分类
  const buildMediaPreviewCategoriesFromBackend = (
    videos: MediaPreviewFile[],
    audios: MediaPreviewFile[],
    images: MediaPreviewFile[]
  ) => {
    const cats: MediaPreviewCategory[] = []
    
    if (videos.length > 0) {
      const formats: Record<string, number> = {}
      videos.forEach(f => {
        formats[f.ext] = (formats[f.ext] || 0) + 1
      })
      cats.push({
        type: 'video',
        label: '视频',
        icon: '🎬',
        files: videos,
        formats,
        expanded: false
      })
    }
    
    if (audios.length > 0) {
      const formats: Record<string, number> = {}
      audios.forEach(f => {
        formats[f.ext] = (formats[f.ext] || 0) + 1
      })
      cats.push({
        type: 'audio',
        label: '音频',
        icon: '🎵',
        files: audios,
        formats,
        expanded: false
      })
    }
    
    if (images.length > 0) {
      const formats: Record<string, number> = {}
      images.forEach(f => {
        formats[f.ext] = (formats[f.ext] || 0) + 1
      })
      cats.push({
        type: 'image',
        label: '图片',
        icon: '🖼️',
        files: images,
        formats,
        expanded: false
      })
    }
    
    mediaPreviewCategories.value = cats
  }

  // 切换媒体预览分类展开状态
  const toggleMediaCategory = (type: string) => {
    const cat = mediaPreviewCategories.value.find(c => c.type === type)
    if (cat) {
      cat.expanded = !cat.expanded
    }
  }

  // 格式化格式统计显示
  const formatFormatsSummary = (formats: Record<string, number>): string => {
    return Object.entries(formats)
      .sort((a, b) => b[1] - a[1])
      .map(([ext, count]) => `.${ext}(${count})`)
      .join(' ')
  }

  // 扁平化目录条目，提取所有文件
  async function flattenEntries(dirPath: string): Promise<{ name: string; path: string; size: number }[]> {
    const files: { name: string; path: string; size: number }[] = []

    async function processDir(dir: string) {
      const entries = await readDir(dir)
      for (const entry of entries) {
        const fullPath = `${dir}/${entry.name}`
        if (entry.isDirectory) {
          await processDir(fullPath)
        } else if (entry.isFile) {
          files.push({ name: entry.name, path: fullPath, size: 0 })
        }
      }
    }

    await processDir(dirPath)
    return files
  }

  const selectAllCategories = () => {
    selectedCategories.value = categories.value.map(c => c.key)
    // Also select all formats for each category
    const allFormats: Record<string, string[]> = {}
    categories.value.forEach(cat => {
      allFormats[cat.key] = cat.formats.map(f => f.ext)
    })
    selectedFormats.value = allFormats
    isAllSelected.value = true
    isIndeterminate.value = false
  }

  const toggleCategory = (key: string) => {
    // If clicking on already expanded, collapse it
    if (expandedCategory.value === key) {
      expandedCategory.value = null
      return
    }
    expandedCategory.value = key
  }

  const toggleCategorySelection = (key: string) => {
    const cat = categories.value.find(c => c.key === key)
    if (!cat) return

    const idx = selectedCategories.value.indexOf(key)
    if (idx > -1) {
      // Deselect category and all its formats
      selectedCategories.value.splice(idx, 1)
      selectedFormats.value[key] = []
    } else {
      // Select category and all its formats
      selectedCategories.value.push(key)
      selectedFormats.value[key] = cat.formats.map(f => f.ext)
    }
    updateSelectState()
  }

  const toggleFormat = (category: string, ext: string) => {
    if (!selectedFormats.value[category]) {
      selectedFormats.value[category] = []
    }
    
    const formats = selectedFormats.value[category]!
    const idx = formats.indexOf(ext)
    if (idx > -1) {
      formats.splice(idx, 1)
    } else {
      formats.push(ext)
    }
    
    // Update category selection state based on format selection
    const cat = categories.value.find(c => c.key === category)
    if (cat) {
      const selectedCount = formats.length
      const catIdx = selectedCategories.value.indexOf(category)
      
      if (selectedCount === 0) {
        // Remove category if no formats selected
        if (catIdx > -1) selectedCategories.value.splice(catIdx, 1)
      } else if (catIdx === -1) {
        // Add category if any format selected
        selectedCategories.value.push(category)
      }
    }
    
    updateSelectState()
  }

  const selectAllFormatsInCategory = (category: string) => {
    const cat = categories.value.find(c => c.key === category)
    if (!cat) return
    
    selectedFormats.value[category] = cat.formats.map(f => f.ext)
    if (!selectedCategories.value.includes(category)) {
      selectedCategories.value.push(category)
    }
    updateSelectState()
  }

  const deselectAllFormatsInCategory = (category: string) => {
    selectedFormats.value[category] = []
    const idx = selectedCategories.value.indexOf(category)
    if (idx > -1) {
      selectedCategories.value.splice(idx, 1)
    }
    updateSelectState()
  }

  const isCategoryFullySelected = (category: string) => {
    const cat = categories.value.find(c => c.key === category)
    if (!cat) return false
    const selected = selectedFormats.value[category] || []
    return selected.length === cat.formats.length
  }

  const isCategoryPartiallySelected = (category: string) => {
    const cat = categories.value.find(c => c.key === category)
    if (!cat) return false
    const selected = selectedFormats.value[category] || []
    return selected.length > 0 && selected.length < cat.formats.length
  }

  const isFormatSelected = (category: string, ext: string) => {
    return selectedFormats.value[category]?.includes(ext) || false
  }

  const getSelectedFormatCount = (category: string) => {
    return selectedFormats.value[category]?.length || 0
  }

  const updateSelectState = () => {
    const totalCats = categories.value.length
    const selectedCats = selectedCategories.value.length
    
    // Check if all formats in all categories are selected
    let allFormatsSelected = true
    let anyFormatSelected = false
    
    categories.value.forEach(cat => {
      const selected = selectedFormats.value[cat.key] || []
      if (selected.length > 0) anyFormatSelected = true
      if (selected.length < cat.formats.length) allFormatsSelected = false
    })
    
    isAllSelected.value = allFormatsSelected && selectedCats === totalCats
    isIndeterminate.value = anyFormatSelected && !isAllSelected.value
  }

  const handleSelectAllChange = (val: boolean) => {
    if (val) {
      selectAllCategories()
    } else {
      selectedCategories.value = []
      selectedFormats.value = {}
      isAllSelected.value = false
      isIndeterminate.value = false
    }
  }
  
  const getEffectiveIncludePatterns = () => {
    if (isAllSelected.value) return includePatterns.value
    
    const exts: string[] = []
    Object.entries(selectedFormats.value).forEach(([_cat, formats]) => {
      exts.push(...formats)
    })
    
    if (exts.length === 0) return includePatterns.value
    
    // Convert extensions to wildcard patterns like "*.jpg" (backend matches filename only)
    return exts.map(e => `*.${e}`)
  }

  // Get selected extensions sorted by count (descending)
  const getSelectedExtsSorted = () => {
    const extCounts: { ext: string; count: number }[] = []
    
    categories.value.forEach(cat => {
      const selectedExts = selectedFormats.value[cat.key] || []
      cat.formats.forEach(fmt => {
        if (selectedExts.includes(fmt.ext)) {
          extCounts.push({ ext: fmt.ext, count: fmt.count })
        }
      })
    })
    
    // Sort by count descending
    return extCounts.sort((a, b) => b.count - a.count).map(item => item.ext)
  }

  // Generate output dir name based on selected formats
  const generateOutputDirName = (basePath: string) => {
    if (isAllSelected.value) {
      return `${basePath}_汇总`
    }
    
    const selectedExts = getSelectedExtsSorted()
    
    if (selectedExts.length === 0) {
      return `${basePath}_汇总`
    }
    
    if (selectedExts.length <= 3) {
      // Format: baseName_ext1_ext2_ext3汇总
      return `${basePath}_${selectedExts.join('_')}汇总`
    } else {
      // More than 3 formats, use simple naming
      return `${basePath}_汇总`
    }
  }

  return {
    inputDir, outputDir, organize, minSize, maxSize,
    includePatterns, excludePatterns,
    mediaOnlyMode, mediaFiles,
    running, progress, result, problemLog,
    // Preview
    previewing, previewStats, categories, selectedCategories, selectedFormats,
    expandedCategory,
    isAllSelected, isIndeterminate,
    handlePreview, toggleCategory, toggleCategorySelection, toggleFormat,
    selectAllFormatsInCategory, deselectAllFormatsInCategory,
    isCategoryFullySelected, isCategoryPartiallySelected,
    isFormatSelected, getSelectedFormatCount,
    handleSelectAllChange, getEffectiveIncludePatterns,
    updateSelectState,
    generateOutputDirName,
    // 媒体预览
    mediaPreviewCategories,
    toggleMediaCategory,
    formatFormatsSummary
  }
}
