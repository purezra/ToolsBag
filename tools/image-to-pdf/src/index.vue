<template>
  <div class="image-to-pdf-tool">
    <!-- 步骤1: 文件夹选择 -->
    <div class="section folder-section">
      <h3>📁 选择图片文件夹</h3>
      <div class="folder-input">
        <button class="btn btn-primary" @click="selectFolder" :disabled="loading">
          {{ loading ? '分析中...' : '选择文件夹' }}
        </button>
        <span v-if="folderPath" class="folder-path">{{ folderPath }}</span>
      </div>
    </div>

    <!-- 步骤2: 图片分析结果 -->
    <div v-if="analysis" class="section analysis-section">
      <h3>📊 图片分析结果</h3>
      <div class="analysis-info">
        <div class="stat">
          <span class="label">总数量:</span>
          <span class="value">{{ analysis.images.length }} 张</span>
        </div>
        <div class="stat">
          <span class="label">竖图:</span>
          <span class="value">{{ analysis.portrait_count }} 张</span>
        </div>
        <div class="stat">
          <span class="label">横图:</span>
          <span class="value">{{ analysis.landscape_count }} 张</span>
        </div>
        <div class="stat">
          <span class="label">总大小:</span>
          <span class="value">{{ formatFileSize(analysis.total_size) }}</span>
        </div>
        <div class="stat">
          <span class="label">建议方向:</span>
          <span class="value suggestion">{{ ORIENTATION_NAMES[analysis.suggested_orientation as PageOrientation] || analysis.suggested_orientation }}</span>
        </div>
      </div>
    </div>

    <!-- 步骤3: 页面设置 -->
    <div v-if="analysis" class="section settings-section">
      <h3>⚙️ 页面设置</h3>
      
      <div class="setting-group">
        <label>页面尺寸</label>
        <div class="radio-group">
          <label v-for="(name, key) in PAGE_SIZE_NAMES" :key="key" class="radio-label">
            <input type="radio" v-model="settings.pageSize" :value="key" @change="updatePreview">
            {{ name }}
          </label>
        </div>
      </div>

      <div class="setting-group">
        <label>页面方向</label>
        <div class="radio-group">
          <label v-for="(name, key) in ORIENTATION_NAMES" :key="key" class="radio-label">
            <input type="radio" v-model="settings.orientation" :value="key" @change="updatePreview">
            {{ name }}
          </label>
        </div>
      </div>

      <div class="setting-group">
        <label>边距 (mm)</label>
        <div class="margin-input">
          <div class="preset-buttons">
            <button 
              v-for="m in MARGIN_PRESETS" 
              :key="m" 
              :class="['preset-btn', { active: settings.margin === m }]"
              @click="setMargin(m)"
            >
              {{ m }}mm
            </button>
          </div>
          <div class="custom-margin">
            <input 
              type="number" 
              v-model.number="settings.margin" 
              min="0" 
              :max="maxMargin"
              step="0.5"
              @change="updatePreview"
            >
            <span>mm</span>
          </div>
        </div>
        <p class="hint" v-if="marginError">⚠️ {{ marginError }}</p>
      </div>

      <div class="setting-group">
        <label>压缩比例: {{ settings.compression }}%</label>
        <input 
          type="range" 
          v-model.number="settings.compression" 
          min="10" 
          max="100" 
          step="5"
        >
        <p class="hint">100% = 不压缩，保持原始质量</p>
      </div>
    </div>

    <!-- 步骤4: 预览区域 -->
    <div v-if="previewData" class="section preview-section">
      <h3>👁️ 预览 <span class="preview-hint">(点击缩略图切换页面)</span></h3>
      <div class="preview-container">
        <div class="preview-sidebar">
          <div 
            v-for="(page, index) in previewData.pages.slice(0, 20)" 
            :key="index"
            :class="['thumbnail', { active: currentPreviewPage === index }]"
            @click="selectPreviewPage(index)"
          >
            <div 
              class="page-frame"
              :style="getPageFrameStyle(page)"
            >
              <div 
                class="image-placeholder"
                :style="getImagePlaceholderStyle(page)"
              ></div>
            </div>
            <span class="page-num">{{ index + 1 }}</span>
          </div>
          <div v-if="previewData.pages.length > 20" class="more-pages">
            +{{ previewData.pages.length - 20 }} 页
          </div>
        </div>
        <div class="preview-main">
          <canvas ref="previewCanvas" class="preview-canvas"></canvas>
          <div class="preview-loading" v-if="loadingThumbnail">加载中...</div>
          <div class="preview-info" v-if="previewData.pages[currentPreviewPage]">
            <span>页面 {{ currentPreviewPage + 1 }} / {{ previewData.pages.length }}</span>
            <span>{{ previewData.pages[currentPreviewPage]?.page_width.toFixed(1) }} × {{ previewData.pages[currentPreviewPage]?.page_height.toFixed(1) }} mm</span>
          </div>
        </div>
      </div>
    </div>

    <!-- 步骤5: 生成PDF -->
    <div v-if="analysis" class="section generate-section">
      <h3>📄 生成 PDF</h3>
      <div class="generate-controls">
        <button 
          class="btn btn-success btn-large" 
          @click="generatePdf" 
          :disabled="generating || !canGenerate"
        >
          {{ generating ? '生成中...' : '合成 PDF' }}
        </button>
      </div>

      <!-- 进度显示 -->
      <div v-if="progress" class="progress-container">
        <div class="progress-bar">
          <div class="progress-fill" :style="{ width: progressPercent + '%' }"></div>
        </div>
        <div class="progress-text">
          {{ progress.phase }}: {{ progress.current }} / {{ progress.total }}
        </div>
      </div>

      <!-- 生成结果 -->
      <div v-if="result" class="result-container" :class="{ success: result.success, error: !result.success }">
        <div v-if="result.success">
          <p>✅ PDF 生成成功!</p>
          <div class="result-stats">
            <span>页数: {{ result.page_count }}</span>
            <span>文件大小: {{ formatFileSize(result.file_size) }}</span>
            <span>原始大小: {{ formatFileSize(result.original_total_size) }}</span>
            <span>耗时: {{ (result.elapsed_ms / 1000).toFixed(2) }}s</span>
          </div>
          <p class="output-path">{{ result.output_path }}</p>
        </div>
        <div v-else>
          <p>❌ 生成失败: {{ result.error }}</p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onBeforeUnmount } from 'vue'
import { open, save } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import {
  analyzeFolderForPdf,
  calculatePreviewLayout,
  generatePdf as apiGeneratePdf,
  getImageThumbnail,
  formatFileSize,
  PAGE_SIZE_NAMES,
  ORIENTATION_NAMES,
  MARGIN_PRESETS,
  type PageSize,
  type PageOrientation,
  type FolderAnalysis,
  type PreviewData,
  type LayoutResult,
  type GenerationProgress,
  type GenerationResult
} from './api/image-to-pdf'

// 状态
const folderPath = ref('')
const analysis = ref<FolderAnalysis | null>(null)
const previewData = ref<PreviewData | null>(null)
const currentPreviewPage = ref(0)
const loading = ref(false)
const loadingThumbnail = ref(false)
const generating = ref(false)
const progress = ref<GenerationProgress | null>(null)
const result = ref<GenerationResult | null>(null)
const previewCanvas = ref<HTMLCanvasElement | null>(null)
const thumbnailCache = ref<Map<string, string>>(new Map())
const THUMBNAIL_CACHE_MAX = 50 // 限制缓存大小，避免内存占用过多

// 设置
const settings = ref({
  pageSize: 'A4' as PageSize,
  orientation: 'auto' as PageOrientation,
  margin: 10,
  compression: 100,
  transparentMode: 'flatten_to_white' as 'flatten_to_white' | 'preserve',
  jpegQuality: 90
})

// 计算属性
const maxMargin = computed(() => {
  const sizes: Record<PageSize, number> = {
    A4: 210 * 0.4,
    A3: 297 * 0.4,
    B5: 176 * 0.4,
    ipad_pro: 160.4 * 0.4
  }
  return Math.floor(sizes[settings.value.pageSize])
})

const marginError = computed(() => {
  if (settings.value.margin < 0) return '边距不能为负'
  if (settings.value.margin > maxMargin.value) {
    return `边距不能超过 ${maxMargin.value}mm`
  }
  return ''
})

const canGenerate = computed(() => {
  return analysis.value && analysis.value.images.length > 0 && !marginError.value
})

const progressPercent = computed(() => {
  if (!progress.value) return 0
  return Math.round((progress.value.current / progress.value.total) * 100)
})

// 方法
async function selectFolder() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择包含图片的文件夹'
  })
  
  if (selected && typeof selected === 'string') {
    folderPath.value = selected
    await analyzeFolder()
  }
}

async function analyzeFolder() {
  if (!folderPath.value) return
  
  loading.value = true
  result.value = null
  thumbnailCache.value.clear()
  
  try {
    analysis.value = await analyzeFolderForPdf(folderPath.value)
    
    // 根据建议设置方向
    if (analysis.value.suggested_orientation) {
      settings.value.orientation = analysis.value.suggested_orientation as PageOrientation
    }
    
    await updatePreview()
  } catch (e: any) {
    alert('分析失败: ' + e)
    analysis.value = null
  } finally {
    loading.value = false
  }
}

async function updatePreview() {
  if (!analysis.value) return
  
  try {
    previewData.value = await calculatePreviewLayout(
      analysis.value.images,
      settings.value.pageSize,
      settings.value.orientation,
      settings.value.margin
    )
    
    currentPreviewPage.value = 0
    await nextTick()
    // 首次只绘制框架，不加载缩略图
    drawPreviewFrame()
  } catch (e: any) {
    console.error('预览失败:', e)
  }
}

function setMargin(m: number) {
  settings.value.margin = m
  updatePreview()
}

// 缩略图样式计算
function getPageFrameStyle(page: LayoutResult) {
  const scale = 50 / Math.max(page.page_width, page.page_height)
  return {
    width: `${page.page_width * scale}px`,
    height: `${page.page_height * scale}px`
  }
}

function getImagePlaceholderStyle(page: LayoutResult) {
  const scale = 50 / Math.max(page.page_width, page.page_height)
  return {
    left: `${page.image.x * scale}px`,
    top: `${page.image.y * scale}px`,
    width: `${page.image.scaled_width * scale}px`,
    height: `${page.image.scaled_height * scale}px`
  }
}

// 绘制预览框架（不含图片）
function drawPreviewFrame() {
  if (!previewCanvas.value || !previewData.value) return
  
  const canvas = previewCanvas.value
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  
  const page = previewData.value.pages[currentPreviewPage.value]
  if (!page) return
  
  // 计算缩放比例，使预览适应容器
  const maxWidth = 400
  const maxHeight = 500
  const scale = Math.min(maxWidth / page.page_width, maxHeight / page.page_height)
  
  canvas.width = page.page_width * scale
  canvas.height = page.page_height * scale
  
  // 绘制页面背景
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(0, 0, canvas.width, canvas.height)
  
  // 绘制边距区域（虚线）
  const margin = page.margin * scale
  ctx.strokeStyle = '#cccccc'
  ctx.setLineDash([4, 4])
  ctx.strokeRect(margin, margin, canvas.width - 2 * margin, canvas.height - 2 * margin)
  ctx.setLineDash([])
  
  // 绘制图片占位区域
  const imgX = page.image.x * scale
  const imgY = page.image.y * scale
  const imgW = page.image.scaled_width * scale
  const imgH = page.image.scaled_height * scale
  
  ctx.fillStyle = '#f0f0f0'
  ctx.fillRect(imgX, imgY, imgW, imgH)
  ctx.strokeStyle = '#ddd'
  ctx.strokeRect(imgX, imgY, imgW, imgH)
  
  // 显示尺寸信息
  ctx.fillStyle = '#999'
  ctx.font = '12px sans-serif'
  ctx.textAlign = 'center'
  ctx.textBaseline = 'middle'
  ctx.fillText(`${page.image.original_width} × ${page.image.original_height}`, imgX + imgW / 2, imgY + imgH / 2)
}

// 选择预览页面并加载缩略图
async function selectPreviewPage(index: number) {
  currentPreviewPage.value = index
  drawPreviewFrame()
  await loadAndDrawThumbnail()
}

// 加载并绘制缩略图
async function loadAndDrawThumbnail() {
  if (!previewCanvas.value || !previewData.value || !analysis.value) return
  
  const canvas = previewCanvas.value
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  
  const page = previewData.value.pages[currentPreviewPage.value]
  if (!page) return
  
  const imgInfo = analysis.value.images[currentPreviewPage.value]
  if (!imgInfo) return
  
  // 检查缓存
  let thumbnail = thumbnailCache.value.get(imgInfo.path)
  
  if (!thumbnail) {
    loadingThumbnail.value = true
    try {
      thumbnail = await getImageThumbnail(imgInfo.path, 512)
      // 缓存大小限制：超出时删除最早的条目
      if (thumbnailCache.value.size >= THUMBNAIL_CACHE_MAX) {
        const firstKey = thumbnailCache.value.keys().next().value
        if (firstKey) thumbnailCache.value.delete(firstKey)
      }
      thumbnailCache.value.set(imgInfo.path, thumbnail)
    } catch (e) {
      console.error('加载缩略图失败:', e)
      loadingThumbnail.value = false
      return
    }
    loadingThumbnail.value = false
  }
  
  // 绘制缩略图
  const maxWidth = 400
  const maxHeight = 500
  const scale = Math.min(maxWidth / page.page_width, maxHeight / page.page_height)
  
  const imgX = page.image.x * scale
  const imgY = page.image.y * scale
  const imgW = page.image.scaled_width * scale
  const imgH = page.image.scaled_height * scale
  
  const img = new Image()
  img.onload = () => {
    ctx.drawImage(img, imgX, imgY, imgW, imgH)
  }
  img.src = thumbnail
}

async function generatePdf() {
  if (!analysis.value || !canGenerate.value) return
  
  // 选择保存路径
  const savePath = await save({
    title: '保存 PDF',
    defaultPath: 'output.pdf',
    filters: [{ name: 'PDF', extensions: ['pdf'] }]
  })
  
  if (!savePath) return
  
  generating.value = true
  progress.value = null
  result.value = null
  
  try {
    result.value = await apiGeneratePdf(analysis.value.images, {
      page_size: settings.value.pageSize,
      orientation: settings.value.orientation,
      margin: settings.value.margin,
      compression: settings.value.compression,
      output_path: savePath,
      transparent_mode: settings.value.transparentMode,
      jpeg_quality: settings.value.jpegQuality
    })
  } catch (e: any) {
    result.value = {
      success: false,
      output_path: '',
      file_size: 0,
      original_total_size: 0,
      page_count: 0,
      elapsed_ms: 0,
      error: String(e)
    }
  } finally {
    generating.value = false
    progress.value = null
  }
}

// 监听进度事件
let unlistenProgress: (() => void) | null = null

onMounted(async () => {
  unlistenProgress = await listen<GenerationProgress>('pdf_progress', (event) => {
    progress.value = event.payload
  })
})

onBeforeUnmount(() => {
  unlistenProgress?.()
  thumbnailCache.value.clear()
})
</script>

<style scoped>
.image-to-pdf-tool {
  padding: 20px;
  max-width: 900px;
  margin: 0 auto;
}

.section {
  background: #f8f9fa;
  border-radius: 8px;
  padding: 16px;
  margin-bottom: 16px;
}

.section h3 {
  margin: 0 0 12px 0;
  font-size: 16px;
  color: #333;
}

.preview-hint {
  font-size: 12px;
  color: #888;
  font-weight: normal;
}

/* 文件夹选择 */
.folder-input {
  display: flex;
  align-items: center;
  gap: 12px;
}

.folder-path {
  color: #666;
  font-size: 13px;
  word-break: break-all;
}

/* 分析结果 */
.analysis-info {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
}

.stat {
  display: flex;
  align-items: center;
  gap: 6px;
}

.stat .label {
  color: #666;
}

.stat .value {
  font-weight: 500;
  color: #333;
}

.stat .suggestion {
  color: #2196f3;
}

/* 设置 */
.setting-group {
  margin-bottom: 16px;
}

.setting-group > label {
  display: block;
  margin-bottom: 8px;
  font-weight: 500;
  color: #333;
}

.radio-group {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
}

.radio-label {
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
}

.margin-input {
  display: flex;
  gap: 16px;
  align-items: center;
}

.preset-buttons {
  display: flex;
  gap: 8px;
}

.preset-btn {
  padding: 4px 12px;
  border: 1px solid #ddd;
  border-radius: 4px;
  background: white;
  cursor: pointer;
  transition: background 0.2s, color 0.2s, border-color 0.2s;
}

.preset-btn.active {
  background: #2196f3;
  color: white;
  border-color: #2196f3;
}

.custom-margin {
  display: flex;
  align-items: center;
  gap: 4px;
}

.custom-margin input {
  width: 60px;
  padding: 4px 8px;
  border: 1px solid #ddd;
  border-radius: 4px;
}

.hint {
  font-size: 12px;
  color: #888;
  margin-top: 4px;
}

/* 预览 */
.preview-container {
  display: flex;
  gap: 16px;
}

.preview-sidebar {
  width: 70px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 500px;
  overflow-y: auto;
}

.thumbnail {
  cursor: pointer;
  text-align: center;
  padding: 4px;
  border-radius: 4px;
  transition: background 0.2s;
}

.thumbnail:hover {
  background: #e0e0e0;
}

.thumbnail.active {
  background: #bbdefb;
}

.page-frame {
  background: white;
  border: 1px solid #ccc;
  position: relative;
  margin: 0 auto;
}

.image-placeholder {
  position: absolute;
  background: #e8e8e8;
}

.page-num {
  font-size: 10px;
  color: #666;
}

.more-pages {
  text-align: center;
  font-size: 11px;
  color: #888;
  padding: 6px;
}

.preview-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  position: relative;
}

.preview-canvas {
  border: 1px solid #ddd;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
}

.preview-loading {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  background: rgba(0,0,0,0.7);
  color: white;
  padding: 8px 16px;
  border-radius: 4px;
  font-size: 12px;
}

.preview-info {
  margin-top: 8px;
  display: flex;
  gap: 16px;
  font-size: 13px;
  color: #666;
}

/* 生成 */
.generate-controls {
  margin-bottom: 16px;
}

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
  transition: opacity 0.2s;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-primary {
  background: #2196f3;
  color: white;
}

.btn-primary:hover:not(:disabled) {
  background: #1976d2;
}

.btn-success {
  background: #4caf50;
  color: white;
}

.btn-success:hover:not(:disabled) {
  background: #388e3c;
}

.btn-large {
  padding: 12px 32px;
  font-size: 16px;
}

/* 进度 */
.progress-container {
  margin-bottom: 16px;
}

.progress-bar {
  height: 8px;
  background: #e0e0e0;
  border-radius: 4px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: #4caf50;
  transition: width 0.3s;
}

.progress-text {
  margin-top: 8px;
  font-size: 13px;
  color: #666;
}

/* 结果 */
.result-container {
  padding: 16px;
  border-radius: 8px;
}

.result-container.success {
  background: #e8f5e9;
}

.result-container.error {
  background: #ffebee;
}

.result-stats {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  margin: 8px 0;
  font-size: 13px;
  color: #666;
}

.output-path {
  font-size: 12px;
  color: #888;
  word-break: break-all;
}
</style>
