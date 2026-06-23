<template>
  <div class="img2pdf">
    <!-- 顶部操作栏：路径 + 分析 + 合成 PDF -->
    <div class="tb-toolbar pdf-toolbar">
      <el-input
        v-model="pathInput"
        :placeholder="t('粘贴文件夹路径或图片路径，回车分析')"
        clearable
        :disabled="loading"
        class="path-input"
        @keyup.enter="handlePathSubmit"
      >
        <template #prepend>
          <el-button :icon="FolderOpened" :loading="loading" @click="selectFolder">
            {{ t('选择') }}
          </el-button>
        </template>
        <template #append>
          <el-button :loading="loading" :disabled="!pathInput.trim()" @click="handlePathSubmit">
            {{ t('分析') }}
          </el-button>
        </template>
      </el-input>
      <el-checkbox v-model="recursive" :disabled="loading" @change="onRecursiveChange">
        {{ t('递归') }}
      </el-checkbox>
      <el-button
        type="success"
        :icon="MagicStick"
        @click="generatePdfAction"
        :disabled="generating || !canGenerate"
        :loading="generating"
      >{{ generating ? t('生成中') : t('合成 PDF') }}</el-button>
      <span v-if="analysis" class="pdf-toolbar__hint">
        {{ analysis.images.length }} {{ t('张') }} · {{ formatFileSize(analysis.total_size) }}
      </span>
    </div>

    <!-- 实际扫描路径 / 自动切换提示 -->
    <p v-if="analysis?.auto_switched_from_file" class="hint is-warn toolbar-hint">
      {{ t('检测到的是文件路径，已自动切换为其所在目录并按非递归扫描') }}
    </p>
    <p v-if="analysis" class="hint toolbar-hint">
      {{ t('实际扫描') }}：{{ analysis.effective_path }}
      <el-tag v-if="analysis.recursive" size="small" type="info" effect="plain">{{ t('递归') }}</el-tag>
    </p>

    <!-- 生成进度 / 结果（内联） -->
    <div v-if="progress" class="progress-container">
      <el-progress :percentage="progressPercent" :text-inside="true" :stroke-width="18" />
      <div class="progress-text">
        {{ progress.phase }}: {{ progress.current }} / {{ progress.total }}
        <span v-if="progress.current_file" class="current-file">— {{ shortFileName(progress.current_file) }}</span>
      </div>
    </div>
    <div v-if="result" class="result-container" :class="{ success: result.success, error: !result.success, warn: result.success && result.exceeded_target }">
      <div v-if="result.success">
        <p class="result-title">
          {{ result.exceeded_target ? t('PDF 已生成（超出体积上限）') : t('PDF 生成成功') }}
        </p>
        <div class="result-stats">
          <span>{{ t('页数') }}: {{ result.page_count }}</span>
          <span>{{ t('文件大小') }}: {{ formatFileSize(result.file_size) }}</span>
          <span>{{ t('原始大小') }}: {{ formatFileSize(result.original_total_size) }}</span>
          <span>{{ t('体积比') }}: {{ Math.round(result.size_ratio * 100) }}%</span>
          <span>{{ t('耗时') }}: {{ (result.elapsed_ms / 1000).toFixed(2) }}s</span>
        </div>
        <p class="mode-used">{{ t('实际参数') }}: {{ result.mode_used }}</p>
        <p v-if="result.exceeded_target" class="warn-note">
          {{ t('已尝试最低质量档（q=75）但仍超过设定上限。可改用便携模式手动设置目标占比以获得更小体积。') }}
        </p>
        <div v-if="result.warnings && result.warnings.length" class="result-warnings">
          <p class="warn-note">{{ t('已跳过以下文件：') }}</p>
          <div v-for="(w, i) in result.warnings" :key="i" class="warning-item">· {{ w }}</div>
        </div>
        <p class="output-path">{{ result.output_path }}</p>
      </div>
      <div v-else>
        <p class="result-title">{{ t('生成失败') }}: {{ result.error }}</p>
      </div>
    </div>

    <!-- 页面设置（可折叠，默认收起；仅分析后显示） -->
    <div v-if="analysis" class="tb-collapse">
      <el-collapse>
        <el-collapse-item>
          <template #title>
            {{ t('页面设置') }}
            <span class="tb-collapse-summary">{{ pageSettingsSummary }}</span>
          </template>

          <div class="setting-group">
            <label>{{ t('页面模式') }}</label>
            <el-radio-group v-model="settings.pageModeType" size="small" @change="onPageModeChange">
              <el-radio-button label="original">{{ t('原图大小') }}</el-radio-button>
              <el-radio-button label="fixed">{{ t('固定幅面') }}</el-radio-button>
            </el-radio-group>
            <p class="hint" v-if="settings.pageModeType === 'original'">
              {{ t('每页尺寸 = 原图像素（72 DPI），横图横摆竖图竖摆，无边距') }}
            </p>
          </div>

          <template v-if="settings.pageModeType === 'fixed'">
            <div class="setting-group">
              <label>{{ t('页面尺寸') }}</label>
              <el-radio-group v-model="settings.pageSize" size="small" @change="updatePreview">
                <el-radio-button v-for="(name, key) in PAGE_SIZE_NAMES" :key="key" :label="key">
                  {{ name }}
                </el-radio-button>
              </el-radio-group>
            </div>

            <div class="setting-group">
              <label>{{ t('幅面方向') }}</label>
              <el-radio-group v-model="settings.fixedOrientation" size="small" @change="updatePreview">
                <el-radio-button v-for="(name, key) in FIXED_ORIENTATION_NAMES" :key="key" :label="key">
                  {{ name }}
                </el-radio-button>
              </el-radio-group>
            </div>

            <div class="setting-group">
              <label>{{ t('边距 (mm)') }}</label>
              <div class="margin-input">
                <div class="preset-buttons">
                  <el-button
                    v-for="m in MARGIN_PRESETS"
                    :key="m"
                    size="small"
                    :type="settings.margin === m ? 'primary' : 'default'"
                    :plain="settings.margin !== m"
                    @click="setMargin(m)"
                  >{{ m }}mm</el-button>
                </div>
                <el-input-number
                  v-model="settings.margin"
                  size="small"
                  :min="0"
                  :max="maxMargin"
                  :step="0.5"
                  @change="updatePreview"
                />
              </div>
              <p class="hint is-danger" v-if="marginError">{{ marginError }}</p>
            </div>
          </template>

          <div class="setting-group">
            <label>{{ t('合并模式') }}</label>
            <el-radio-group v-model="settings.mergeMode" size="small">
              <el-radio-button label="lossless">{{ t('无损模式') }}</el-radio-button>
              <el-radio-button label="portable">{{ t('便携模式') }}</el-radio-button>
            </el-radio-group>

            <div v-if="settings.mergeMode === 'lossless'" class="mode-detail">
              <p class="hint">
                {{ t('原始 JPEG 直接嵌入（零重编码），其他格式以 q=95 编码。') }}
                <br />
                {{ t('结果体积超过原图总大小') }} <strong>{{ Math.round(settings.maxSizeRatio * 100) }}%</strong>
                {{ t('时自动降级到 q=85 / q=75。') }}
              </p>
              <div class="ratio-row">
                <span>{{ t('体积上限：原图的') }}</span>
                <el-slider
                  v-model="settings.maxSizeRatio"
                  :min="1.05"
                  :max="2.00"
                  :step="0.05"
                  :format-tooltip="(v: number) => `${Math.round(v * 100)}%`"
                  class="ratio-slider"
                />
                <strong>{{ Math.round(settings.maxSizeRatio * 100) }}%</strong>
              </div>
            </div>

            <div v-else class="mode-detail">
              <p class="hint">{{ t('用滑块设置目标体积占比。100% 接近原图大小，10% 大幅压缩。') }}</p>
              <div class="ratio-row">
                <span>{{ t('目标占比：原图的') }}</span>
                <el-slider
                  v-model="settings.portableTargetRatio"
                  :min="0.10"
                  :max="1.00"
                  :step="0.05"
                  :format-tooltip="(v: number) => `${Math.round(v * 100)}%`"
                  class="ratio-slider"
                />
                <strong>{{ Math.round(settings.portableTargetRatio * 100) }}%</strong>
              </div>
            </div>
          </div>
        </el-collapse-item>
      </el-collapse>
    </div>

    <!-- 分析结果（可折叠，默认展开；仅分析后显示） -->
    <div v-if="analysis" class="tb-collapse">
      <el-collapse v-model="analysisCollapse">
        <el-collapse-item name="analysis">
          <template #title>
            {{ t('图片分析结果') }}
            <el-tag type="success" effect="plain" size="small" class="collapse-tag">{{ analysis.images.length }} {{ t('张') }}</el-tag>
          </template>

          <div class="stats-row">
            <div class="stat">
              <span class="label">{{ t('总数量') }}</span>
              <span class="value">{{ analysis.images.length }}</span>
            </div>
            <div class="stat">
              <span class="label">{{ t('总大小') }}</span>
              <span class="value">{{ formatFileSize(analysis.total_size) }}</span>
            </div>
            <div class="stat">
              <span class="label">{{ t('建议方向') }}</span>
              <span class="value suggestion">
                {{ FIXED_ORIENTATION_NAMES[analysis.suggested_orientation as FixedOrientation] || analysis.suggested_orientation }}
              </span>
            </div>
          </div>

          <div v-if="metaWarnings.length" class="meta-warnings">
            <p v-for="w in metaWarnings" :key="w" class="meta-warn">{{ w }}</p>
          </div>

          <DistCard :title="t('方向分布')" :rows="orientationRows" />
          <DistCard :title="t('格式分布')" :rows="formatRows" />
          <DistCard v-if="resolutionRows.length" :title="t('分辨率分布（按长边）')" :rows="resolutionRows" />
        </el-collapse-item>
      </el-collapse>
    </div>

    <!-- 跳过的文件（可折叠，仅异常时显示） -->
    <div v-if="analysis?.skipped?.length" class="tb-collapse">
      <el-collapse>
        <el-collapse-item>
          <template #title>
            {{ t('跳过的文件') }}
            <el-tag type="warning" effect="plain" size="small" class="collapse-tag">{{ analysis.skipped.length }} {{ t('个') }}</el-tag>
          </template>
          <div class="skipped-list">
            <div v-for="sf in analysis.skipped" :key="sf.path" class="skipped-row">
              <span class="skipped-ext">.{{ sf.ext }}</span>
              <span class="skipped-reason">{{ sf.reason }}</span>
              <span class="skipped-name" :title="sf.path">{{ shortFileName(sf.path) }}</span>
            </div>
          </div>
        </el-collapse-item>
      </el-collapse>
    </div>

    <!-- 预览面板（全宽） -->
    <main class="preview-panel" v-if="previewData">
      <header class="preview-toolbar">
        <div class="tool-group">
          <span class="tool-label">{{ t('网格') }}</span>
          <el-radio-group v-model="previewGridSize" size="small">
            <el-radio-button :label="4">2×2</el-radio-button>
            <el-radio-button :label="9">3×3</el-radio-button>
            <el-radio-button :label="16">4×4</el-radio-button>
          </el-radio-group>
        </div>

        <div class="tool-group">
          <span class="tool-label">{{ t('模式') }}</span>
          <el-radio-group v-model="previewMode" size="small">
            <el-radio-button label="page">{{ t('翻页') }}</el-radio-button>
            <el-radio-button label="waterfall">{{ t('瀑布流') }}</el-radio-button>
            <el-radio-button label="auto">{{ t('自动播放') }}</el-radio-button>
            <el-radio-button label="list">{{ t('列表') }}</el-radio-button>
          </el-radio-group>
        </div>

        <div v-if="previewMode === 'auto'" class="tool-group interval-group">
          <span class="tool-label">{{ t('间隔') }}</span>
          <el-slider
            v-model="autoPlayInterval"
            :min="0.1"
            :max="5"
            :step="0.1"
            :format-tooltip="(v: number) => `${v.toFixed(1)}s`"
            class="interval-slider"
            size="small"
          />
          <span class="interval-display">{{ autoPlayInterval.toFixed(1) }}s</span>
        </div>

        <div v-if="previewMode === 'page' || previewMode === 'auto'" class="tool-group page-nav">
          <el-button size="small" :disabled="gridPageIndex === 0" @click="prevGridPage" circle>
            ‹
          </el-button>
          <span class="page-counter">{{ gridPageIndex + 1 }} / {{ totalGridPages }}</span>
          <el-button size="small" :disabled="gridPageIndex >= totalGridPages - 1" @click="nextGridPage" circle>
            ›
          </el-button>
        </div>

        <div class="tool-group spacer"></div>

        <div class="tool-group">
          <span class="page-info">
            {{ t('共') }} {{ previewData.pages.length }} {{ t('页') }}
          </span>
        </div>
      </header>

      <div
        ref="gridScroller"
        class="grid-scroller"
        :class="{ waterfall: previewMode === 'waterfall', list: previewMode === 'list' }"
      >
        <div v-if="previewMode === 'list'" class="file-list-view">
          <el-table
            :data="fileListRows"
            height="100%"
            stripe
            size="small"
            @row-click="(row: FileListRow) => onCellClick(row.index - 1)"
          >
            <el-table-column prop="index" label="#" width="58" align="right" />
            <el-table-column prop="name" :label="t('文件名')" min-width="220" show-overflow-tooltip />
            <el-table-column prop="resolution" :label="t('分辨率')" width="120" align="center" />
            <el-table-column prop="orientation" :label="t('方向')" width="78" align="center" />
            <el-table-column prop="format" :label="t('格式')" width="82" align="center" />
            <el-table-column prop="size" :label="t('大小')" width="100" align="right" />
            <el-table-column prop="bitDepth" :label="t('位深')" width="76" align="center" />
            <el-table-column prop="flags" :label="t('提示')" min-width="160" show-overflow-tooltip />
          </el-table>
        </div>

        <template v-else>
          <div
            class="grid-container"
            :style="gridContainerStyle"
          >
            <div
              v-for="item in visibleCells"
              :key="`${item.pageIdx}-${item.page.image_path}`"
              class="grid-cell"
              :class="{ active: previewMode !== 'waterfall' && currentSelectedPage === item.pageIdx }"
              @click="onCellClick(item.pageIdx)"
            >
              <div class="cell-frame-wrap">
                <div
                  class="cell-frame"
                  :class="{ 'no-page-decor': isImageFillsPage(item.page) }"
                  :style="cellFrameStyle(item.page)"
                >
                  <div
                    v-if="item.page.margin > 0"
                    class="cell-margin-box"
                    :style="cellMarginStyle(item.page)"
                  ></div>
                  <img
                    v-if="thumbnails.get(item.page.image_path)"
                    :src="thumbnails.get(item.page.image_path)"
                    class="cell-img"
                    :style="cellImageBoxStyle(item.page)"
                    loading="lazy"
                  />
                  <div
                    v-else
                    class="cell-placeholder"
                    :style="cellImageBoxStyle(item.page)"
                  >
                    <span>{{ item.page.image.original_width }}×{{ item.page.image.original_height }}</span>
                  </div>
                </div>
              </div>
              <div class="cell-meta">
                <span class="cell-num">{{ item.pageIdx + 1 }}</span>
                <span class="cell-name">{{ shortFileName(item.page.image_path) }}</span>
              </div>
            </div>
          </div>

          <div v-if="previewMode === 'waterfall' && previewData.pages.length > waterfallLimit" class="waterfall-more">
            <el-button size="small" @click="waterfallLimit += 60">
              {{ t('再加载 60 项') }}（{{ previewData.pages.length - waterfallLimit }} {{ t('剩余') }}）
            </el-button>
          </div>
        </template>
      </div>
    </main>

    <main v-else class="preview-panel placeholder-panel">
      <div class="placeholder-empty">
        <p>{{ t('选择文件夹后将在此处预览') }}</p>
        <p class="hint">{{ t('支持 JPG/PNG/WebP/TIFF/BMP/GIF/TGA/DDS/PNM/QOI/HDR/ICO 等位图格式混合合并') }}</p>
      </div>
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onBeforeUnmount, watch, h, defineComponent, type PropType, type StyleValue } from 'vue'
import { ElMessage } from 'element-plus'
import { FolderOpened, MagicStick } from '@element-plus/icons-vue'
import { open, save } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { hasTauriRuntime } from '@core/utils/tauri'
import { useSettings } from '@core/hooks/useSettings'
import {
  analyzeFolderForPdf,
  calculatePreviewLayout,
  generatePdf as apiGeneratePdf,
  getImageThumbnail,
  formatFileSize,
  PAGE_SIZE_NAMES,
  FIXED_ORIENTATION_NAMES,
  MARGIN_PRESETS,
  DEFAULT_LOSSLESS_MAX_RATIO,
  DEFAULT_PORTABLE_TARGET_RATIO,
  type PageSize,
  type FixedOrientation,
  type PageMode,
  type MergeMode,
  type FolderAnalysis,
  type PreviewData,
  type LayoutResult,
  type GenerationProgress,
  type GenerationResult,
} from './api/image-to-pdf'

const { t } = useSettings()

// ==================== Distribution row component (inline) ====================

interface DistRow {
  key: string
  label: string
  count: number
  percent: number
  color: string
}

const DistCard = defineComponent({
  name: 'DistCard',
  props: {
    title: { type: String, required: true },
    rows: { type: Array as PropType<DistRow[]>, required: true },
  },
  setup(props) {
    return () =>
      h('div', { class: 'dist-card' }, [
        h('p', { class: 'dist-title' }, props.title),
        h(
          'div',
          { class: 'dist-list' },
          props.rows.map((row) =>
            h('div', { class: 'dist-row', key: row.key }, [
              h('span', { class: 'dist-label' }, row.label),
              h('div', { class: 'dist-bar' }, [
                h('div', {
                  class: 'dist-bar-fill',
                  style: { width: row.percent + '%', background: row.color },
                }),
              ]),
              h('span', { class: 'dist-value' }, `${row.count} · ${row.percent}%`),
            ]),
          ),
        ),
      ])
  },
})

// ==================== State ====================

const pathInput = ref('')
const recursive = ref(false)
const analysis = ref<FolderAnalysis | null>(null)
const previewData = ref<PreviewData | null>(null)
const loading = ref(false)
const generating = ref(false)
const progress = ref<GenerationProgress | null>(null)
const result = ref<GenerationResult | null>(null)

// 缩略图缓存（path → data URL）
const thumbnails = ref<Map<string, string>>(new Map())
const THUMBNAIL_CACHE_MAX = 200
const loadingThumbnails = ref<Set<string>>(new Set())

// 预览控件状态
const previewGridSize = ref<4 | 9 | 16>(9)
const previewMode = ref<'page' | 'waterfall' | 'auto' | 'list'>('page')
const autoPlayInterval = ref<number>(2.0)
const gridPageIndex = ref(0)
const currentSelectedPage = ref(0)
const waterfallLimit = ref(60)
const gridScroller = ref<HTMLDivElement | null>(null)

// 设置
const settings = ref({
  pageModeType: 'original' as 'original' | 'fixed',
  pageSize: 'A4' as PageSize,
  fixedOrientation: 'auto' as FixedOrientation,
  margin: 10,
  mergeMode: 'lossless' as 'lossless' | 'portable',
  maxSizeRatio: DEFAULT_LOSSLESS_MAX_RATIO,
  portableTargetRatio: DEFAULT_PORTABLE_TARGET_RATIO,
})

// ==================== Computed ====================

const maxMargin = computed(() => {
  const sizes: Record<PageSize, number> = {
    A4: 210 * 0.4, A3: 297 * 0.4, B5: 176 * 0.4, ipad_pro: 160.4 * 0.4,
  }
  return Math.floor(sizes[settings.value.pageSize])
})

const marginError = computed(() => {
  if (settings.value.pageModeType === 'original') return ''
  if (settings.value.margin < 0) return t('边距不能为负')
  if (settings.value.margin > maxMargin.value) return `${t('边距不能超过')} ${maxMargin.value}mm`
  return ''
})

const canGenerate = computed(
  () => analysis.value && analysis.value.images.length > 0 && !marginError.value,
)

const progressPercent = computed(() => {
  if (!progress.value || progress.value.total === 0) return 0
  return Math.round((progress.value.current / progress.value.total) * 100)
})

// Chart palette resolves to per-skin CSS variables so colors adapt to light/dark
// and each skin family (defined in core/assets/styles/variables.css).
const PALETTE = [
  'var(--chart-1)', 'var(--chart-2)', 'var(--chart-3)', 'var(--chart-4)', 'var(--chart-5)',
  'var(--chart-6)', 'var(--chart-7)', 'var(--chart-8)', 'var(--chart-9)', 'var(--chart-10)',
]

const orientationRows = computed<DistRow[]>(() => {
  if (!analysis.value) return []
  const a = analysis.value
  const total = a.images.length || 1
  const rows: DistRow[] = []
  const items: [string, string, number, string][] = [
    ['portrait', t('竖图'), a.portrait_count, 'var(--chart-1)'],
    ['landscape', t('横图'), a.landscape_count, 'var(--chart-2)'],
  ]
  if (a.square_count > 0) items.push(['square', t('方图'), a.square_count, 'var(--chart-3)'])
  for (const [key, label, count, color] of items) {
    rows.push({ key, label, count, percent: Math.round((count / total) * 100), color })
  }
  return rows
})

const formatRows = computed<DistRow[]>(() => {
  if (!analysis.value) return []
  const counts: Record<string, number> = {}
  for (const img of analysis.value.images) counts[img.format] = (counts[img.format] || 0) + 1
  const total = analysis.value.images.length || 1
  const sorted = Object.entries(counts).sort((a, b) => b[1] - a[1])
  return sorted.map(([fmt, count], i) => ({
    key: fmt,
    label: fmt.toUpperCase(),
    count,
    percent: Math.round((count / total) * 100),
    color: PALETTE[i % PALETTE.length] ?? PALETTE[0]!,
  }))
})

const resolutionRows = computed<DistRow[]>(() => {
  if (!analysis.value) return []
  const total = analysis.value.images.length || 1
  return analysis.value.resolution_buckets
    .slice()
    .reverse()
    .map((b, i) => ({
      key: String(b.min_long_edge),
      label: b.label,
      count: b.count,
      percent: Math.round((b.count / total) * 100),
      color: PALETTE[i % PALETTE.length] ?? PALETTE[0]!,
    }))
})

const metaWarnings = computed<string[]>(() => {
  if (!analysis.value) return []
  const warns: string[] = []
  const a = analysis.value as any
  if (a.apng_count > 0) {
    warns.push(`${a.apng_count} ${t('张 APNG（动画 PNG），PDF 仅取首帧')}`)
  }
  if (a.bit16_count > 0) {
    warns.push(`${a.bit16_count} ${t('张 16-bit PNG，已降为 8-bit（视觉通常无明显影响，非严格无损）')}`)
  }
  if (a.gamma_count > 0) {
    warns.push(`${a.gamma_count} ${t('张含非 sRGB gamma 信息，可能轻微偏色')}`)
  }
  if (a.icc_count > 0) {
    warns.push(`${a.icc_count} ${t('张含 ICC 色彩配置，PDF 未内嵌 ICC，广色域图可能偏色')}`)
  }
  if (a.long_image_count > 0) {
    warns.push(`${a.long_image_count} ${t('张长图（长宽比>5:1），可能生成超长页面，建议开启分页或缩放')}`)
  }
  return warns
})

// 分析结果折叠区默认展开
const analysisCollapse = ref<string[]>(['analysis'])

// 页面设置折叠标题摘要
const pageSettingsSummary = computed(() => {
  const s = settings.value
  const mode = s.pageModeType === 'original' ? t('原图大小') : t('固定幅面')
  const merge = s.mergeMode === 'lossless' ? t('无损') : t('便携')
  return `${mode} · ${merge}`
})

interface FileListRow {
  index: number
  name: string
  path: string
  resolution: string
  orientation: string
  format: string
  size: string
  bitDepth: string
  flags: string
}

const fileListRows = computed<FileListRow[]>(() => {
  if (!analysis.value) return []
  return analysis.value.images.map((img, idx) => {
    const isSquare = Math.abs(img.width - img.height) <= Math.max(img.width, img.height) * 0.05
    const orientation = isSquare ? t('方图') : img.height > img.width ? t('竖图') : t('横图')
    const flags: string[] = []
    if (img.has_alpha) flags.push('Alpha')
    if (img.is_apng) flags.push('APNG首帧')
    if (img.bit_depth && img.bit_depth > 8) flags.push(`${img.bit_depth}-bit→8-bit`)
    if (img.has_gamma) flags.push('Gamma')
    if (img.is_grayscale) flags.push('灰度')
    return {
      index: idx + 1,
      name: shortFileName(img.path),
      path: img.path,
      resolution: `${img.width}×${img.height}`,
      orientation,
      format: img.format.toUpperCase(),
      size: formatFileSize(img.file_size || 0),
      bitDepth: img.bit_depth ? `${img.bit_depth}` : '-',
      flags: flags.join(' · ') || '-',
    }
  })
})

const totalGridPages = computed(() => {
  if (!previewData.value) return 0
  return Math.max(1, Math.ceil(previewData.value.pages.length / previewGridSize.value))
})

interface GridCell {
  pageIdx: number
  page: LayoutResult
}

const visibleCells = computed<GridCell[]>(() => {
  if (!previewData.value) return []
  const pages = previewData.value.pages
  if (previewMode.value === 'waterfall') {
    return pages.slice(0, waterfallLimit.value).map((page, i) => ({ pageIdx: i, page }))
  }
  // page / auto: paginate by gridSize
  const start = gridPageIndex.value * previewGridSize.value
  const end = Math.min(start + previewGridSize.value, pages.length)
  const cells: GridCell[] = []
  for (let i = start; i < end; i++) {
    const page = pages[i]
    if (page) cells.push({ pageIdx: i, page })
  }
  return cells
})

const gridContainerStyle = computed<StyleValue>(() => {
  const grid = previewGridSize.value
  const cols = grid === 4 ? 2 : grid === 9 ? 3 : 4
  return { '--grid-cols': cols }
})

// ==================== Methods ====================

function shortFileName(p: string): string {
  if (!p) return ''
  const segs = p.split(/[\\/]/)
  return segs[segs.length - 1] || p
}

function buildPageMode(): PageMode {
  if (settings.value.pageModeType === 'original') return 'original'
  return { fixed: { orientation: settings.value.fixedOrientation } }
}

function buildMergeMode(): MergeMode {
  if (settings.value.mergeMode === 'lossless') {
    return { lossless: { max_size_ratio: settings.value.maxSizeRatio } }
  }
  return { portable: { target_ratio: settings.value.portableTargetRatio } }
}

async function selectFolder() {
  const selected = await open({ directory: true, multiple: false, title: t('选择包含图片的文件夹') })
  if (selected && typeof selected === 'string') {
    pathInput.value = selected
    await analyzeFolderAction()
  }
}

function handlePathSubmit() {
  if (pathInput.value.trim()) analyzeFolderAction()
}

function onRecursiveChange() {
  if (analysis.value) analyzeFolderAction()
}

function onPageModeChange() {
  updatePreview()
}

async function analyzeFolderAction() {
  const p = pathInput.value.trim()
  if (!p) return

  loading.value = true
  result.value = null
  thumbnails.value.clear()
  loadingThumbnails.value.clear()
  gridPageIndex.value = 0
  waterfallLimit.value = 60

  try {
    analysis.value = await analyzeFolderForPdf(p, recursive.value)
    if (analysis.value.auto_switched_from_file) {
      pathInput.value = analysis.value.effective_path
      recursive.value = false
    }
    if (analysis.value.suggested_orientation) {
      settings.value.fixedOrientation = analysis.value.suggested_orientation as FixedOrientation
    }
    await updatePreview()
  } catch (e: any) {
    ElMessage.error(t('分析失败') + ': ' + (e?.message || e))
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
      buildPageMode(),
      settings.value.margin,
    )
    gridPageIndex.value = 0
    currentSelectedPage.value = 0
    await nextTick()
    schedulePreloadVisible()
  } catch (e: any) {
    console.error('预览失败:', e)
    ElMessage.error(t('预览更新失败') + ': ' + (e?.message || e))
  }
}

function setMargin(m: number) {
  settings.value.margin = m
  updatePreview()
}

// ---- Cell layout (相对单元格的百分比定位) ----

/// 图片是否完全填充页面（无边距、无留白）
/// 在这种情况下隐藏页面装饰（边框、阴影、白底），让预览更贴近实际 PDF 渲染
function isImageFillsPage(page: LayoutResult): boolean {
  if (page.margin > 0) return false
  // 浮点容差：scaled = page 即视为铺满
  const tol = 0.5  // mm
  const widthFills = Math.abs(page.image.scaled_width - page.page_width) < tol
  const heightFills = Math.abs(page.image.scaled_height - page.page_height) < tol
  return widthFills && heightFills
}

function cellFrameStyle(page: LayoutResult) {
  // 页框保持原始高宽比，由父容器决定大小
  return { aspectRatio: `${page.page_width} / ${page.page_height}` }
}

function cellImageBoxStyle(page: LayoutResult) {
  // 图片相对于页面的百分比定位，contain 缩放后的位置
  const left = (page.image.x / page.page_width) * 100
  const top = (page.image.y / page.page_height) * 100
  const width = (page.image.scaled_width / page.page_width) * 100
  const height = (page.image.scaled_height / page.page_height) * 100
  return {
    left: `${left}%`,
    top: `${top}%`,
    width: `${width}%`,
    height: `${height}%`,
  }
}

function cellMarginStyle(page: LayoutResult) {
  if (page.margin <= 0) return { display: 'none' }
  const m = page.margin
  const left = (m / page.page_width) * 100
  const top = (m / page.page_height) * 100
  return {
    left: `${left}%`,
    top: `${top}%`,
    right: `${left}%`,
    bottom: `${top}%`,
  }
}

// ---- Thumbnail loading ----

async function loadThumbnail(path: string): Promise<void> {
  if (thumbnails.value.has(path) || loadingThumbnails.value.has(path)) return
  loadingThumbnails.value.add(path)
  try {
    const data = await getImageThumbnail(path, 384)
    // LRU eviction
    if (thumbnails.value.size >= THUMBNAIL_CACHE_MAX) {
      const firstKey = thumbnails.value.keys().next().value
      if (firstKey) thumbnails.value.delete(firstKey)
    }
    thumbnails.value.set(path, data)
  } catch (e) {
    console.warn('缩略图加载失败:', path, e)
  } finally {
    loadingThumbnails.value.delete(path)
  }
}

// 并行预加载当前可见单元的缩略图（最多 8 个并发）
function schedulePreloadVisible() {
  const cells = visibleCells.value
  const tasks = cells
    .map((c) => c.page.image_path)
    .filter((p) => !thumbnails.value.has(p) && !loadingThumbnails.value.has(p))

  // 简单并发控制：每次最多启动 8 个
  const MAX_CONCURRENT = 8
  let running = 0
  const queue = [...tasks]
  function next() {
    while (running < MAX_CONCURRENT && queue.length) {
      const path = queue.shift()!
      running++
      loadThumbnail(path).finally(() => {
        running--
        next()
      })
    }
  }
  next()
}

// ---- Page navigation ----

function prevGridPage() {
  if (gridPageIndex.value > 0) {
    gridPageIndex.value--
    schedulePreloadVisible()
  }
}

function nextGridPage() {
  if (gridPageIndex.value < totalGridPages.value - 1) {
    gridPageIndex.value++
    schedulePreloadVisible()
  }
}

function onCellClick(pageIdx: number) {
  currentSelectedPage.value = pageIdx
}

// ---- Auto-play ----

let autoTimer: ReturnType<typeof setInterval> | null = null

function startAutoPlay() {
  stopAutoPlay()
  if (previewMode.value !== 'auto') return
  const ms = Math.max(100, Math.round(autoPlayInterval.value * 1000))
  autoTimer = setInterval(() => {
    if (totalGridPages.value <= 1) return
    gridPageIndex.value = (gridPageIndex.value + 1) % totalGridPages.value
    schedulePreloadVisible()
  }, ms)
}

function stopAutoPlay() {
  if (autoTimer) {
    clearInterval(autoTimer)
    autoTimer = null
  }
}

watch([previewMode, autoPlayInterval], () => {
  startAutoPlay()
})

watch(previewGridSize, () => {
  // 切换网格大小时回到第一页
  gridPageIndex.value = 0
  nextTick(() => schedulePreloadVisible())
})

watch(previewMode, () => {
  // 切换模式时重置位置
  if (previewMode.value === 'waterfall') {
    waterfallLimit.value = 60
  }
  nextTick(() => schedulePreloadVisible())
})

watch(visibleCells, () => {
  schedulePreloadVisible()
})

watch(waterfallLimit, () => {
  if (previewMode.value === 'waterfall') schedulePreloadVisible()
})

// ---- Generate ----

async function generatePdfAction() {
  if (!analysis.value || !canGenerate.value) return
  const savePath = await save({
    title: t('保存 PDF'),
    defaultPath: 'output.pdf',
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  })
  if (!savePath) return

  generating.value = true
  progress.value = null
  result.value = null

  try {
    result.value = await apiGeneratePdf(analysis.value.images, {
      page_size: settings.value.pageSize,
      page_mode: buildPageMode(),
      margin: settings.value.margin,
      output_path: savePath,
      merge_mode: buildMergeMode(),
    })
  } catch (e: any) {
    result.value = {
      success: false, output_path: '', file_size: 0, original_total_size: 0,
      page_count: 0, elapsed_ms: 0, mode_used: '', size_ratio: 0,
      exceeded_target: false, warnings: [], error: String(e?.message || e),
    }
  } finally {
    generating.value = false
    progress.value = null
  }
}

// ---- Lifecycle ----

let unlistenProgress: (() => void) | null = null

onMounted(async () => {
  if (!hasTauriRuntime()) return
  unlistenProgress = await listen<GenerationProgress>('pdf_progress', (e) => {
    progress.value = e.payload
  })
})

onBeforeUnmount(() => {
  unlistenProgress?.()
  stopAutoPlay()
  thumbnails.value.clear()
})
</script>

<style scoped>
/* ============================ Layout ============================ */

.img2pdf {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  overflow: hidden;
  gap: 10px;
}
.img2pdf :deep(.tb-collapse) {
  flex-shrink: 0;
}

.pdf-toolbar {
  flex-shrink: 0;
}
.path-input {
  flex: 1;
  min-width: 220px;
  max-width: 520px;
}
.pdf-toolbar__hint {
  font-size: 12px;
  color: var(--text-muted);
  margin-left: auto;
  white-space: nowrap;
}
.toolbar-hint {
  margin: 0;
  font-size: 12px;
}

/* 预览面板（全宽，纵向流中占主区） */
.preview-panel {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--card-border);
  border-radius: var(--card-radius);
  background: var(--bg-secondary);
}

.placeholder-panel {
  align-items: center;
  justify-content: center;
}

.placeholder-empty {
  color: var(--text-muted);
  font-size: 14px;
  text-align: center;
}
.placeholder-empty .hint {
  margin-top: 6px;
}

/* 窄屏：允许整页滚动，预览区给最小高度 */
@media (max-width: 820px) {
  .img2pdf {
    overflow-y: auto;
  }
  .preview-panel {
    min-height: 360px;
  }
}

/* ============================ Cards ============================ */

/* ============================ Shared ============================ */

.hint { font-size: 11px; color: var(--text-muted); margin: 4px 0 0; line-height: 1.5; }
.hint.is-danger { color: var(--danger); }
.hint.is-warn { color: var(--warning); }

/* 折叠标题里的 tag 与摘要 */
.collapse-tag { margin-left: 8px; }

/* 滑块在折叠区/工具栏内的弹性宽度 */
.ratio-slider { flex: 1; min-width: 120px; margin: 0 12px; }
.interval-slider { width: 140px; }

/* ============================ Skipped ============================ */

.skipped-list { display: flex; flex-direction: column; gap: 4px; max-height: 160px; overflow-y: auto; }
.skipped-row {
  display: flex; align-items: center; gap: 8px; padding: 4px 8px;
  border-radius: var(--radius-xs); background: var(--bg-tertiary); font-size: 11px;
}
.skipped-ext {
  flex-shrink: 0;
  min-width: 44px; padding: 1px 6px; border-radius: 3px;
  background: var(--warning); color: var(--text-inverse);
  font-weight: 600; font-size: 10px; text-align: center;
}
.skipped-reason { color: var(--text-secondary); flex: 1; }
.skipped-name {
  flex-shrink: 0;
  color: var(--text-muted); font-family: var(--font-mono); font-size: 10px;
  max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}

/* ============================ Stats ============================ */

.stats-row {
  display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 6px;
  margin-bottom: 8px;
}

.stat {
  display: flex; flex-direction: column; gap: 2px;
  padding: 6px 8px; border-radius: var(--radius-sm); background: var(--bg-tertiary);
}
.stat .label { color: var(--text-secondary); font-size: 11px; }
.stat .value { font-weight: 600; color: var(--text-primary); font-size: 13px; }
.stat .suggestion { color: var(--accent); }

.meta-warnings { margin: 4px 0 8px; }
.meta-warn {
  margin: 2px 0; padding: 4px 8px; font-size: 11px;
  background: var(--warning-light); color: var(--warning);
  border-radius: var(--radius-xs); line-height: 1.5;
}

/* ============================ Distribution ============================ */

:deep(.dist-card) {
  padding: 8px 10px; border-radius: var(--radius-sm); background: var(--bg-tertiary); margin-bottom: 6px;
}
:deep(.dist-title) { margin: 0 0 6px; font-size: 11px; font-weight: 600; color: var(--text-secondary); }
:deep(.dist-list) { display: flex; flex-direction: column; gap: 4px; }
:deep(.dist-row) { display: flex; align-items: center; gap: 8px; font-size: 11px; }
:deep(.dist-label) { min-width: 80px; color: var(--text-primary); font-weight: 500; }
:deep(.dist-bar) {
  flex: 1; height: 7px; border-radius: 4px;
  background: var(--bg-secondary); overflow: hidden;
}
:deep(.dist-bar-fill) {
  height: 100%; border-radius: 4px;
  transition: width 0.3s ease; min-width: 2px;
}
:deep(.dist-value) {
  min-width: 88px; text-align: right; color: var(--text-muted); font-size: 10px;
}

/* ============================ Settings ============================ */

.setting-group { margin-bottom: 10px; }
.setting-group > label {
  display: block; margin-bottom: 6px; font-weight: 500; color: var(--text-secondary); font-size: 12px;
}
.margin-input { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
.preset-buttons { display: flex; gap: 4px; flex-wrap: wrap; }

.mode-detail {
  margin-top: 6px; padding: 6px 8px; border-radius: var(--radius-sm); background: var(--bg-tertiary);
}
.ratio-row {
  display: flex; align-items: center; margin-top: 6px; font-size: 11px; color: var(--text-secondary);
}
.ratio-row strong { min-width: 44px; text-align: right; color: var(--text-primary); }

/* ============================ Generate ============================ */

.progress-container { margin-top: 10px; }
.progress-text { margin-top: 6px; font-size: 11px; color: var(--text-secondary); }
.current-file {
  color: var(--text-muted); font-family: var(--font-mono); font-size: 10px;
}

.result-container {
  margin-top: 10px; padding: 8px 10px; border-radius: var(--radius-md);
  border: 1px solid var(--border-secondary);
}
.result-container.success { background: var(--success-light); }
.result-container.error { background: var(--danger-light); }
.result-container.warn {
  background: var(--warning-light); border-color: var(--warning);
}
.result-title { margin: 0 0 6px; font-weight: 600; color: var(--text-primary); font-size: 12px; }
.result-stats {
  display: flex; flex-wrap: wrap; gap: 8px; margin: 6px 0;
  font-size: 11px; color: var(--text-secondary);
}
.mode-used { font-size: 10px; color: var(--text-muted); margin: 2px 0; }
.warn-note { font-size: 10px; color: var(--warning); margin: 2px 0; }
.result-warnings { margin-top: 6px; padding: 6px 8px; background: rgba(230, 162, 60, 0.08); border-radius: 4px; max-height: 120px; overflow-y: auto; }
.warning-item { font-size: 11px; color: var(--text-secondary); line-height: 1.6; word-break: break-all; }
.output-path { font-size: 10px; color: var(--text-muted); word-break: break-all; margin: 4px 0 0; }

/* ============================ Preview toolbar ============================ */

.preview-toolbar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  background: var(--bg-primary);
  border-bottom: 1px solid var(--border-secondary);
  flex-wrap: wrap;
}

.tool-group {
  display: flex; align-items: center; gap: 6px;
}

.tool-group.spacer { flex: 1; }

.tool-label {
  font-size: 11px; color: var(--text-muted); font-weight: 500;
}

.interval-group { gap: 8px; }
.interval-display {
  font-size: 11px; color: var(--text-secondary);
  font-family: var(--font-mono);
  min-width: 32px; text-align: right;
}

.page-nav {
  align-items: center;
}
.page-counter {
  font-size: 12px; color: var(--text-secondary);
  min-width: 60px; text-align: center;
  font-family: var(--font-mono);
}

.page-info {
  font-size: 11px; color: var(--text-muted);
}

/* ============================ Grid scroller ============================ */

.grid-scroller {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 12px;
  scrollbar-gutter: stable;
}

.grid-scroller.waterfall {
  /* 瀑布流模式：滚轮平滑滚动 */
  scroll-behavior: smooth;
}

.grid-scroller.list {
  padding: 0;
  background: var(--bg-primary);
}

.file-list-view {
  height: 100%;
  min-height: 0;
}

:deep(.file-list-view .el-table) {
  height: 100%;
}

:deep(.file-list-view .el-table__row) {
  cursor: pointer;
}

:deep(.file-list-view .el-table__row:hover > td) {
  background: var(--accent-light) !important;
}

.grid-container {
  display: grid;
  grid-template-columns: repeat(var(--grid-cols, 3), 1fr);
  gap: 12px;
}

/* ============================ Grid cell ============================ */

.grid-cell {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 6px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background 0.15s ease, transform 0.15s ease;
  user-select: none;
}

.grid-cell:hover {
  background: var(--bg-tertiary);
}

.grid-cell.active {
  background: var(--accent-light);
  outline: 1.5px solid var(--accent);
}

.cell-frame-wrap {
  width: 100%;
  flex: 1 1 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 0;
}

.cell-frame {
  position: relative;
  background: var(--tb-paper-bg);
  border: 1px solid var(--tb-paper-border);
  box-shadow: var(--shadow-xs);
  /* aspect-ratio set via inline style */
  max-width: 100%;
  max-height: 100%;
  width: 100%;
  height: auto;
}

/* 原图大小模式（图像完全填充页面）：
   去掉页面装饰，避免给用户"图被嵌入到白页面里"的视觉错觉。
   实际 PDF 中页面尺寸 = 图像尺寸，没有任何边距 / 边框 / 留白。 */
.cell-frame.no-page-decor {
  background: transparent;
  border: none;
  box-shadow: none;
}

.cell-margin-box {
  position: absolute;
  border: 1px dashed var(--tb-paper-margin);
  pointer-events: none;
}

.cell-img,
.cell-placeholder {
  position: absolute;
  object-fit: contain;
  image-rendering: -webkit-optimize-contrast;
}

.cell-placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--tb-paper-placeholder);
  border: 1px solid var(--tb-paper-border);
  color: var(--tb-paper-placeholder-text);
  font-size: 10px;
  text-align: center;
  overflow: hidden;
}

.cell-meta {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  font-size: 10px;
  color: var(--text-muted);
}

.cell-num {
  flex-shrink: 0;
  background: var(--accent);
  color: var(--text-inverse);
  border-radius: 8px;
  padding: 0 6px;
  min-width: 18px;
  text-align: center;
  font-weight: 600;
  font-size: 9px;
  line-height: 14px;
}

.cell-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.waterfall-more {
  display: flex;
  justify-content: center;
  margin: 16px 0 8px;
}
</style>
