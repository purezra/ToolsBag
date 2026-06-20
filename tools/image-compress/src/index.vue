<template>
  <div class="image-compress-tool">
    <aside class="left-panel">
      <section class="card hero-card" @dragover.prevent @drop.prevent="handleDrop">
        <p class="eyebrow">Image Compress</p>
        <h3>{{ t('图片压缩') }}</h3>
        <p class="hint">{{ t('纯本地离线处理，支持 JPG / PNG，输出 AVIF / JXL') }}</p>
        <div class="action-row">
          <el-button type="primary" :icon="Picture" @click="selectImages">{{ t('选择图片') }}</el-button>
          <el-button :icon="FolderOpened" @click="selectFolder">{{ t('选择文件夹') }}</el-button>
        </div>
        <el-checkbox v-model="recursive" class="mt8">{{ t('递归扫描子目录') }}</el-checkbox>
        <p class="drop-hint">{{ t('也可以把图片或文件夹拖到这里') }}</p>
      </section>

      <section class="card">
        <header class="card-head">
          <h3>{{ t('输出设置') }}</h3>
          <el-checkbox v-model="config.advanced_mode">{{ t('高级模式') }}</el-checkbox>
        </header>

        <template v-if="!config.advanced_mode">
          <div class="setting-group">
            <label>{{ t('智能输出') }}</label>
            <p class="hint strong">{{ t('JPG/JPEG 自动输出 JXL；PNG 自动输出 AVIF。') }}</p>
            <p class="hint">{{ t('默认推荐：严格无损 + 均衡编码 + 保留元数据。') }}</p>
          </div>

          <div class="setting-group">
            <label>{{ t('压缩模式') }}</label>
            <el-radio-group v-model="config.mode" size="small">
              <el-radio-button label="lossless">{{ t('严格无损') }}</el-radio-button>
              <el-radio-button label="near_lossless">{{ t('视觉无损') }}</el-radio-button>
            </el-radio-group>
            <p class="hint" v-if="config.mode === 'lossless'">{{ t('JPEG→JXL 使用原始 JPEG 无损封装；PNG→AVIF 使用最高质量无损设置。') }}</p>
            <p class="hint" v-else>{{ t('肉眼几乎不可辨，通常能获得更高压缩率。') }}</p>
          </div>

          <div class="setting-group">
            <label>{{ t('编码强度') }}</label>
            <el-radio-group v-model="config.effort_preset" size="small">
              <el-radio-button label="fast">{{ t('快速') }}</el-radio-button>
              <el-radio-button label="balanced">{{ t('均衡') }}</el-radio-button>
              <el-radio-button label="best">{{ t('最佳压缩') }}</el-radio-button>
            </el-radio-group>
            <p class="hint">{{ effortHint }}</p>
          </div>

          <div class="setting-group">
            <label>{{ t('元数据') }}</label>
            <el-radio-group v-model="config.metadata_policy" size="small">
              <el-radio-button label="keep">{{ t('保留') }}</el-radio-button>
              <el-radio-button label="strip">{{ t('剥离') }}</el-radio-button>
            </el-radio-group>
            <p class="hint">{{ t('保留适合摄影归档；剥离可减小体积并移除拍摄/GPS 等附加信息。JPEG→JXL 原始封装支持该策略，常规像素编码会尽量避免写入额外元数据。') }}</p>
          </div>

          <p class="hint" :class="{ 'is-warn': !libjxlStatus?.available }">{{ libjxlStatus?.message || t('正在检测 libjxl/cjxl...') }}</p>
        </template>

        <template v-else>
          <div class="setting-group">
            <label>{{ t('输出格式') }}</label>
            <el-select v-model="config.output_format" style="width: 100%;">
              <el-option :label="t('智能自动（JPG→JXL，PNG→AVIF）')" value="auto" />
              <el-option label="JPEG XL (.jxl)" value="jxl" />
              <el-option label="AVIF (.avif)" value="avif" />
            </el-select>
            <p class="hint" v-if="effectiveFormat === 'jxl'">{{ t('JXL 适合本地归档；JPG/JPEG 可用 libjxl 原始封装。') }}</p>
            <p class="hint" v-else-if="effectiveFormat === 'avif'">{{ t('AVIF 适合网页和跨设备分享，新版浏览器支持较好。') }}</p>
            <p class="hint" v-else>{{ t('按输入格式自动选择输出：JPG/JPEG→JXL，PNG→AVIF。') }}</p>
          </div>

          <div class="setting-group">
            <label>{{ t('压缩模式') }}</label>
            <el-radio-group v-model="config.mode" size="small">
              <el-radio-button label="lossless">{{ t('无损') }}</el-radio-button>
              <el-radio-button label="near_lossless">{{ t('近无损') }}</el-radio-button>
              <el-radio-button label="lossy">{{ t('有损') }}</el-radio-button>
            </el-radio-group>
          </div>

          <div class="setting-group" v-if="config.mode === 'lossy' && effectiveFormat === 'jxl'">
            <label>Distance: {{ config.distance.toFixed(1) }}</label>
            <el-slider v-model="config.distance" :min="1" :max="15" :step="0.5" />
            <p class="hint">{{ t('越大体积越小、质量越低；1.0=视觉无损，3~5 为常用均衡档。') }}</p>
          </div>

          <template v-if="effectiveFormat === 'jxl' || config.output_format === 'auto'">
            <div class="setting-group">
              <label>JXL Effort: {{ config.jxl_effort }}</label>
              <el-slider v-model="config.jxl_effort" :min="1" :max="10" :step="1" />
              <p class="hint">{{ t('1–10，3=快速，7=均衡，9+=极慢；越大压缩率越好。') }}</p>
            </div>
            <el-checkbox v-model="config.jxl_jpeg_lossless" :disabled="!libjxlStatus?.available">{{ t('JPG/JPEG 原始无损封装') }}</el-checkbox>
            <p class="hint" :class="{ 'is-warn': !libjxlStatus?.available }">{{ libjxlStatus?.message || t('正在检测 libjxl/cjxl...') }}</p>
            <el-checkbox v-model="config.keep_hdr">{{ t('保留 16-bit 精度（PNG 等高位深输入）') }}</el-checkbox>
          </template>

          <template v-if="effectiveFormat === 'avif' || config.output_format === 'auto'">
            <p class="hint" v-if="config.mode === 'lossless'">{{ t('AVIF 无损模式固定使用最高质量；16-bit / HDR 会降为 SDR 8-bit。') }}</p>
            <div class="setting-group" v-else>
              <label>{{ t('色彩质量') }}: {{ config.avif_color_quality }}</label>
              <el-slider v-model="config.avif_color_quality" :min="config.mode === 'near_lossless' ? 80 : 1" :max="100" :step="1" />
            </div>
            <div class="setting-group" v-if="config.mode !== 'lossless'">
              <label>{{ t('Alpha 质量') }}: {{ config.avif_alpha_quality }}</label>
              <el-slider v-model="config.avif_alpha_quality" :min="config.mode === 'near_lossless' ? 80 : 1" :max="100" :step="1" />
            </div>
            <div class="setting-group">
              <label>AVIF Speed: {{ config.avif_speed }}</label>
              <el-slider v-model="config.avif_speed" :min="1" :max="10" :step="1" />
              <p class="hint">{{ t('1=最慢最佳压缩，6=均衡，8+=快速。') }}</p>
            </div>
          </template>

          <div class="setting-group">
            <label>{{ t('元数据') }}</label>
            <el-radio-group v-model="config.metadata_policy" size="small">
              <el-radio-button label="keep">{{ t('保留') }}</el-radio-button>
              <el-radio-button label="strip">{{ t('剥离') }}</el-radio-button>
            </el-radio-group>
          </div>
        </template>

        <div class="setting-group">
          <label>{{ t('最大宽高') }}</label>
          <el-input-number v-model="config.max_dimension" :min="0" :max="100000" :precision="0" :step="256" style="width: 100%;" />
          <p class="hint">0 = {{ t('不缩放') }}</p>
        </div>
      </section>

      <section class="card">
        <header class="card-head"><h3>{{ t('预设模板') }}</h3></header>
        <div class="preset-grid">
          <el-button @click="applyPreset('photo')">{{ t('摄影存档') }}</el-button>
          <el-button @click="applyPreset('design')">{{ t('设计原稿') }}</el-button>
          <el-button @click="applyPreset('web')">{{ t('网页分发') }}</el-button>
        </div>
      </section>

      <section class="card">
        <header class="card-head"><h3>{{ t('导出') }}</h3></header>
        <el-input v-model="config.output_dir" :placeholder="t('选择输出目录')" readonly>
          <template #append><el-button @click="selectOutputDir">{{ t('选择') }}</el-button></template>
        </el-input>
        <el-checkbox v-model="config.zip_output" class="mt8">{{ t('完成后打包 ZIP') }}</el-checkbox>
        <el-button type="success" :icon="MagicStick" :disabled="!canStart || running" :loading="running" style="width:100%; margin-top:10px;" @click="startCompress">
          {{ running ? t('转换中') : t('开始转换') }}
        </el-button>
        <div v-if="progress" class="progress-box">
          <el-progress
            :percentage="progressPercent"
            :stroke-width="18"
            :text-inside="true"
            :status="running ? undefined : 'success'"
            :striped="running"
            :striped-flow="running"
            :duration="10"
          />
          <p>{{ progress.phase }}：{{ progress.current }} / {{ progress.total }}</p>
          <p class="current-file" v-if="running && progress.current_file">{{ t('正在处理') }}：{{ currentFileName }}</p>
        </div>
        <div v-if="summary?.zip_path" class="zip-path">
          {{ t('ZIP') }}：{{ summary.zip_path }}
        </div>
      </section>
    </aside>

    <main class="right-panel">
      <!-- 导入栏：选择后未导入时提示并提供导入按钮 -->
      <section v-if="pendingPaths.length && !importing && !files.length" class="import-bar">
        <p>{{ t('已选择') }} {{ pendingPaths.length }} {{ t('项，点击导入生成图片列表') }}</p>
        <el-button type="primary" :icon="Download" @click="doImport">{{ t('导入列表') }}</el-button>
      </section>

      <section class="summary-bar">
        <div><strong>{{ files.length }}</strong><span>{{ t('张图片') }}</span></div>
        <div><strong>{{ formatFileSize(totalSize) }}</strong><span>{{ t('原始总大小') }}</span></div>
        <div v-if="summary"><strong>{{ formatFileSize(summary.output_total_size) }}</strong><span>{{ t('输出总大小') }}</span></div>
        <div v-if="summary"><strong>{{ Math.round(summary.compression_ratio * 100) }}%</strong><span>{{ t('体积比') }}</span></div>
      </section>

      <el-table
        :data="tableRows"
        height="100%"
        stripe
        size="small"
        class="file-table"
        v-loading="importing"
        :element-loading-text="t('正在导入...')"
        @row-click="row => selectedPath = row.path"
      >
        <el-table-column prop="name" :label="t('文件名')" min-width="220" show-overflow-tooltip />
        <el-table-column prop="format" :label="t('格式')" width="80" align="center" />
        <el-table-column prop="resolution" :label="t('分辨率')" width="130" align="center" />
        <el-table-column prop="original" :label="t('原始大小')" width="110" align="right" />
        <el-table-column prop="output" :label="t('输出大小')" width="110" align="right" />
        <el-table-column prop="ratio" :label="t('体积比')" width="90" align="right" />
        <el-table-column prop="status" :label="t('状态')" width="110" align="center" />
        <el-table-column prop="error" :label="t('错误')" min-width="180" show-overflow-tooltip />
        <template #empty>
          <div class="empty-hint">
            <p v-if="!pendingPaths.length">{{ t('选择图片或文件夹后点击导入') }}</p>
          </div>
        </template>
      </el-table>
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Download, FolderOpened, MagicStick, Picture } from '@element-plus/icons-vue'
import { open } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useSettings } from '@core/hooks/useSettings'
import {
  analyzeCompressInputs,
  compressImages,
  getLibjxlStatus,
  formatFileSize,
  type CompressConfig,
  type CompressInput,
  type CompressProgress,
  type CompressResult,
  type CompressSummary,
  type LibjxlStatus,
} from './api/image-compress'

const { t } = useSettings()
const recursive = ref(false)
const files = ref<CompressInput[]>([])
const skipped = ref<string[]>([])
const results = ref<Map<string, CompressResult>>(new Map())
const summary = ref<CompressSummary | null>(null)
const progress = ref<CompressProgress | null>(null)
const running = ref(false)
const selectedPath = ref('')
// 选择/导入两步流程：选完只存路径，点"导入列表"才分析。
const pendingPaths = ref<string[]>([])
const importing = ref(false)
const libjxlStatus = ref<LibjxlStatus | null>(null)

const config = ref<CompressConfig>({
  output_format: 'auto',
  mode: 'lossless',
  effort_preset: 'balanced',
  metadata_policy: 'keep',
  advanced_mode: false,
  quality: 95,
  distance: 3.0,
  keep_exif: false,
  max_dimension: 0,
  output_dir: '',
  zip_output: false,
  jxl_effort: 7,
  jxl_jpeg_lossless: false,
  avif_color_quality: 95,
  avif_alpha_quality: 100,
  avif_speed: 6,
  keep_hdr: false,
})

const totalSize = computed(() => files.value.reduce((s, f) => s + f.file_size, 0))
const canStart = computed(() => files.value.length > 0 && config.value.output_dir.trim().length > 0 && !running)
const progressPercent = computed(() => !progress.value || progress.value.total === 0 ? 0 : Math.round(progress.value.current / progress.value.total * 100))
const currentFileName = computed(() => {
  const p = progress.value?.current_file || ''
  const m = p.match(/[\\/][^\\/]*$/)
  return m ? m[0].slice(1) : p
})

const effectiveFormat = computed(() => {
  if (config.value.output_format !== 'auto') return config.value.output_format
  const hasOnlyPng = files.value.length > 0 && files.value.every(f => f.ext === 'png')
  const hasOnlyJpeg = files.value.length > 0 && files.value.every(f => f.ext === 'jpg' || f.ext === 'jpeg')
  if (hasOnlyPng) return 'avif'
  if (hasOnlyJpeg) return 'jxl'
  return 'auto'
})
const effortHint = computed(() => {
  if (config.value.effort_preset === 'fast') return t('快速：JXL effort=3，AVIF speed=8，速度优先。')
  if (config.value.effort_preset === 'best') return t('最佳压缩：JXL effort=9，AVIF speed=1，速度最慢但体积更小。')
  return t('均衡：JXL effort=7，AVIF speed=6，推荐默认。')
})

watch(
  () => [config.value.output_format, libjxlStatus.value?.available] as const,
  ([format, available]) => {
    if ((format === 'avif' || !available) && format !== 'auto') config.value.jxl_jpeg_lossless = false
    if (format === 'avif') config.value.keep_hdr = false
  },
)

const tableRows = computed(() => files.value.map(f => {
  const r = results.value.get(f.path)
  return {
    path: f.path,
    name: f.name,
    format: f.ext.toUpperCase(),
    resolution: `${f.width}×${f.height}`,
    original: formatFileSize(f.file_size),
    output: r?.success ? formatFileSize(r.output_size) : '-',
    ratio: r?.success ? `${Math.round(r.compression_ratio * 100)}%` : '-',
    status: r ? (r.success ? t('成功') : t('失败')) : t('待处理'),
    error: r?.error || '',
  }
}))

// 导入：分析已选路径，填充图片列表。后端只读尺寸不解码，速度较快。
async function doImport() {
  if (!pendingPaths.value.length || importing.value) return
  importing.value = true
  try {
    const res = await analyzeCompressInputs(pendingPaths.value, recursive.value)
    files.value = res.files
    skipped.value = res.skipped
    results.value.clear()
    summary.value = null
    if (res.skipped.length) ElMessage.warning(`${t('已跳过')} ${res.skipped.length} ${t('个文件')}`)
  } catch (e: any) {
    ElMessage.error(String(e?.message || e))
  } finally {
    importing.value = false
  }
}

async function selectImages() {
  try {
    const selected = await open({ multiple: true, filters: [{ name: 'Images', extensions: ['jpg', 'jpeg', 'png'] }] })
    if (Array.isArray(selected)) pendingPaths.value = selected
    else if (typeof selected === 'string') pendingPaths.value = [selected]
    else return
    // 选择新内容后清空旧列表，等待用户点导入
    files.value = []
    results.value.clear()
    summary.value = null
  } catch (e: any) {
    ElMessage.error(String(e?.message || e))
  }
}

async function selectFolder() {
  try {
    const selected = await open({ directory: true, multiple: false })
    if (typeof selected !== 'string') return
    pendingPaths.value = [selected]
    files.value = []
    results.value.clear()
    summary.value = null
  } catch (e: any) {
    ElMessage.error(String(e?.message || e))
  }
}

async function selectOutputDir() {
  try {
    const selected = await open({ directory: true, multiple: false })
    if (typeof selected === 'string') config.value.output_dir = selected
  } catch (e: any) {
    ElMessage.error(String(e?.message || e))
  }
}

async function handleDrop(e: DragEvent) {
  const paths: string[] = []
  for (const item of Array.from(e.dataTransfer?.files || [])) {
    if ((item as any).path) paths.push((item as any).path)
  }
  if (!paths.length) return
  pendingPaths.value = paths
  files.value = []
  results.value.clear()
  summary.value = null
}

function applyPreset(kind: 'photo' | 'design' | 'web') {
  if (kind === 'photo') Object.assign(config.value, { advanced_mode: false, output_format: 'auto', mode: 'lossless', effort_preset: 'balanced', metadata_policy: 'keep', quality: 98, distance: 1.0, jxl_effort: 7, jxl_jpeg_lossless: !!libjxlStatus.value?.available, keep_exif: false, max_dimension: 0, keep_hdr: true, avif_speed: 6 })
  if (kind === 'design') Object.assign(config.value, { advanced_mode: true, output_format: 'jxl', mode: 'lossless', effort_preset: 'best', metadata_policy: 'keep', quality: 100, distance: 0, jxl_effort: 8, jxl_jpeg_lossless: !!libjxlStatus.value?.available, keep_exif: false, max_dimension: 0, keep_hdr: true })
  if (kind === 'web') Object.assign(config.value, { advanced_mode: false, output_format: 'auto', mode: 'near_lossless', effort_preset: 'balanced', metadata_policy: 'strip', quality: 70, avif_color_quality: 80, avif_alpha_quality: 80, avif_speed: 6, keep_exif: false, max_dimension: 1920, keep_hdr: false })
}

function normalizedConfig(): CompressConfig {
  const maxDimension = Number.isFinite(Number(config.value.max_dimension))
    ? Math.trunc(Number(config.value.max_dimension))
    : 0
  const cfg = {
    ...config.value,
    output_format: config.value.advanced_mode ? config.value.output_format : 'auto',
    mode: config.value.advanced_mode ? config.value.mode : (config.value.mode === 'lossy' ? 'near_lossless' : config.value.mode),
    max_dimension: Math.min(Math.max(maxDimension, 0), 100000),
    keep_hdr: (config.value.output_format === 'jxl' || config.value.output_format === 'auto') && config.value.keep_hdr,
    jxl_jpeg_lossless: (config.value.output_format === 'jxl' || config.value.output_format === 'auto') && !!libjxlStatus.value?.available && (config.value.jxl_jpeg_lossless || !config.value.advanced_mode && config.value.mode === 'lossless'),
    avif_color_quality: Math.min(Math.max(Math.trunc(Number(config.value.avif_color_quality) || 95), 1), 100),
    avif_alpha_quality: Math.min(Math.max(Math.trunc(Number(config.value.avif_alpha_quality) || 100), 1), 100),
    avif_speed: Math.min(Math.max(Math.trunc(Number(config.value.avif_speed) || 6), 1), 10),
    jxl_effort: Math.min(Math.max(Math.trunc(Number(config.value.jxl_effort) || 7), 1), 10),
  }
  return cfg
}

async function startCompress() {
  running.value = true
  summary.value = null
  results.value.clear()
  try {
    const sum = await compressImages(files.value, normalizedConfig())
    summary.value = sum
    const m = new Map<string, CompressResult>()
    for (const r of sum.results) m.set(r.input_path, r)
    results.value = m
    ElMessage.success(`${t('完成')}：${sum.success}/${sum.total}${sum.zip_path ? `，ZIP ${t('已生成')}` : ''}`)
  } catch (e: any) {
    ElMessage.error(String(e?.message || e))
  } finally {
    running.value = false
    progress.value = null
  }
}

let unlistenProgress: (() => void) | null = null
let unlistenDrop: (() => void) | null = null
onMounted(async () => {
  unlistenProgress = await listen<CompressProgress>('image_compress_progress', e => { progress.value = e.payload })
  try {
    unlistenDrop = await getCurrentWebview().onDragDropEvent(event => {
      if (event.payload.type === 'drop') {
        pendingPaths.value = event.payload.paths
        files.value = []
        results.value.clear()
        summary.value = null
      }
    })
  } catch {
    // DOM drop fallback below still works in environments exposing File.path.
  }
  try {
    libjxlStatus.value = await getLibjxlStatus()
  } catch (e: any) {
    libjxlStatus.value = { available: false, message: String(e?.message || e), path: null, version: null }
  }
})
onBeforeUnmount(() => {
  unlistenProgress?.()
  unlistenDrop?.()
})
</script>

<style scoped>
.image-compress-tool { display: flex; gap: 10px; height: 100%; min-height: 0; overflow: hidden; }
.left-panel { flex: 0 0 360px; min-width: 320px; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; }
.right-panel { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; border: 1px solid var(--border-secondary); border-radius: var(--radius-md); overflow: hidden; background: var(--bg-primary); }
.card { padding: 10px 12px; border: 1px solid var(--border-secondary); border-radius: var(--radius-md); background: var(--bg-primary); }
.hero-card { border-style: dashed; }
.eyebrow { margin: 0 0 2px; color: var(--accent); font-size: 11px; font-weight: 700; }
h3 { margin: 0; font-size: 15px; }
.hint, .drop-hint { font-size: 12px; color: var(--text-muted); line-height: 1.5; }
.hint.strong { color: var(--text-secondary); font-weight: 600; }
.hint.is-warn { color: var(--warning, #d46b08); }
.action-row { display: flex; gap: 8px; margin-top: 8px; flex-wrap: wrap; }
.mt8 { margin-top: 8px; }
.card-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
.setting-group { margin-bottom: 12px; }
.setting-group > label { display: block; margin-bottom: 6px; font-size: 12px; color: var(--text-secondary); font-weight: 600; }
.preset-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 6px; }
.progress-box { margin-top: 10px; font-size: 12px; color: var(--text-secondary); }
.summary-bar { display: flex; gap: 10px; padding: 10px 12px; border-bottom: 1px solid var(--border-secondary); flex-wrap: wrap; }
.summary-bar > div { padding: 6px 10px; border-radius: var(--radius-sm); background: var(--bg-tertiary); display: flex; gap: 6px; align-items: baseline; }
.summary-bar strong { color: var(--text-primary); }
.summary-bar span { color: var(--text-muted); font-size: 12px; }
.file-table { flex: 1; min-height: 0; }
.import-bar { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 12px; border-bottom: 1px solid var(--border-secondary); background: var(--bg-tertiary); font-size: 13px; color: var(--text-secondary); }
.empty-hint { padding: 40px 0; color: var(--text-muted); font-size: 13px; text-align: center; }
.current-file { margin-top: 2px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.zip-path { margin-top: 8px; padding: 6px 8px; border-radius: var(--radius-sm); background: var(--bg-tertiary); color: var(--text-muted); font-size: 11px; word-break: break-all; }
@media (max-width: 820px) { .image-compress-tool { flex-direction: column; overflow-y: auto; } .left-panel { flex: 0 0 auto; } .right-panel { min-height: 420px; } }
</style>
