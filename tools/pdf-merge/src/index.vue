<template>
  <div
    class="pdf-merge-tool tool-page"
    @dragover.prevent
    @drop.prevent="onDomDrop"
  >
    <!-- 顶部操作栏 -->
    <div class="merge-toolbar">
      <el-button
        type="primary"
        :icon="DocumentAdd"
        :disabled="importing || merging"
        @click="addFiles"
      >
        {{ t('添加文件') }}
      </el-button>

      <el-select
        v-model="sortMode"
        class="merge-sort"
        size="default"
        :disabled="!items.length || merging"
        @change="applySort"
      >
        <el-option
          v-for="option in sortOptions"
          :key="option.value"
          :label="t(option.label)"
          :value="option.value"
        />
      </el-select>

      <span v-if="items.length" class="merge-count">
        {{ t('共 {n} 个文件 · {pages} 页', { n: items.length, pages: totalPages }) }}
      </span>

      <span class="merge-toolbar__spacer"></span>

      <el-tooltip :content="converterTooltip" placement="bottom">
        <span
          class="converter-hint"
          :class="{ 'is-warn': converter && !converter.available }"
        >
          <span class="converter-dot"></span>
          {{ converterLabel }}
        </span>
      </el-tooltip>

      <el-button
        :icon="Delete"
        :disabled="!items.length || merging"
        @click="clearAll"
      >
        {{ t('清空列表') }}
      </el-button>

      <el-button
        type="success"
        :icon="MagicStick"
        :loading="merging"
        :disabled="!canMerge"
        @click="mergeNow"
      >
        {{ merging ? t('合并中…') : t('开始合并') }}
      </el-button>
    </div>

    <!-- 导入提示 -->
    <div v-if="importing" class="merge-importing">
      <el-icon class="is-loading" :size="16"><Loading /></el-icon>
      <span>{{ t('正在解析文件…（Word 文档首次导入需要转换，可能稍慢）') }}</span>
    </div>

    <!-- 导入失败列表 -->
    <div v-if="importErrors.length" class="merge-import-errors">
      <div class="merge-import-errors__head">
        <span>{{ t('部分文件未能添加') }}</span>
        <button type="button" class="merge-import-errors__close" @click="importErrors = []">×</button>
      </div>
      <div v-for="(error, index) in importErrors" :key="index" class="merge-import-errors__item">
        · {{ error }}
      </div>
    </div>

    <!-- 空状态 / 文件列表 -->
    <div class="merge-body">
      <div v-if="!items.length" class="merge-empty">
        <div class="merge-empty__icon">
          <el-icon :size="38"><DocumentCopy /></el-icon>
        </div>
        <h3 class="merge-empty__title">{{ t('把 PDF、图片或 Word 拖到这里') }}</h3>
        <p class="merge-empty__desc">
          {{ t('支持 PDF、JPG/PNG/WebP 等图片，以及 DOC/DOCX；Word 文档可指定参与合并的页码') }}
        </p>
        <el-button type="primary" :icon="DocumentAdd" :disabled="importing" @click="addFiles">
          {{ t('添加文件') }}
        </el-button>
        <p class="merge-empty__tip">
          {{ t('也可以一次拖入多个文件，之后按页数、修改时间排序或自由拖拽调整顺序') }}
        </p>
      </div>

      <template v-else>
        <div
          v-for="(item, index) in items"
          :key="item.uid"
          class="merge-card"
          :class="{
            'is-dragging': dragIndex === index,
            'is-drop-target': dropIndex === index && dragIndex !== null && dragIndex !== index
          }"
          @dragover.prevent="onDragOver(index)"
          @dragleave="onDragLeave(index)"
          @drop.prevent="onDrop(index, $event)"
        >
          <button
            type="button"
            class="drag-handle"
            draggable="true"
            :title="t('拖拽排序')"
            :aria-label="t('拖拽排序')"
            @dragstart="onDragStart(index, $event)"
            @dragend="resetDrag"
          >
            <el-icon :size="15"><Rank /></el-icon>
          </button>

          <span class="merge-card__index">{{ index + 1 }}</span>

          <div class="merge-card__body">
            <div class="merge-card__head">
              <el-icon class="kind-icon" :class="`kind-icon--${item.kind}`" :size="17">
                <component :is="kindIcon(item.kind)" />
              </el-icon>
              <span class="merge-card__name" :title="item.path">{{ item.name }}</span>
              <span class="kind-chip" :class="`kind-chip--${item.kind}`">{{ kindLabel(item.kind) }}</span>
            </div>

            <div class="merge-card__meta">
              <span>{{ item.page_count }} {{ t('页') }}</span>
              <span>{{ formatFileSize(item.file_size) }}</span>
              <span>{{ t('修改于') }} {{ formatModified(item.modified_ms) }}</span>
              <span v-if="item.width && item.height">{{ item.width }} × {{ item.height }}</span>
            </div>

            <div v-if="item.kind === 'doc'" class="page-picker">
              <span class="page-picker__label">{{ t('参与页') }}</span>
              <div class="page-picker__field" :class="{ 'is-invalid': !!selectionInfo(item).error }">
                <el-input
                  v-model="item.pageSelection"
                  size="small"
                  clearable
                  :disabled="merging"
                  :placeholder="t('留空 = 全部，例如 1,5,7 或 1-3,5')"
                />
              </div>
              <span
                class="page-picker__status"
                :class="{
                  'is-invalid': !!selectionInfo(item).error,
                  'is-accent': selectionInfo(item).custom
                }"
              >
                {{ selectionInfo(item).label }}
              </span>
            </div>
          </div>

          <div class="merge-card__ops">
            <button
              type="button"
              class="merge-op"
              :disabled="index === 0 || merging"
              :title="t('上移')"
              :aria-label="t('上移')"
              @click="moveItem(index, -1)"
            >
              <el-icon :size="14"><ArrowUp /></el-icon>
            </button>
            <button
              type="button"
              class="merge-op"
              :disabled="index === items.length - 1 || merging"
              :title="t('下移')"
              :aria-label="t('下移')"
              @click="moveItem(index, 1)"
            >
              <el-icon :size="14"><ArrowDown /></el-icon>
            </button>
            <button
              type="button"
              class="merge-op is-danger"
              :disabled="merging"
              :title="t('移除')"
              :aria-label="t('移除')"
              @click="removeItem(index)"
            >
              <el-icon :size="14"><Close /></el-icon>
            </button>
          </div>
        </div>

        <button type="button" class="merge-add-more" :disabled="importing || merging" @click="addFiles">
          <el-icon :size="15"><DocumentAdd /></el-icon>
          <span>{{ t('继续添加文件') }}</span>
        </button>
      </template>
    </div>

    <!-- 合并进度 -->
    <div v-if="merging" class="merge-progress">
      <el-progress :percentage="progressPercent" :stroke-width="8" :show-text="false" />
      <div class="merge-progress__text">
        <span>{{ progressPhaseLabel }}</span>
        <span v-if="progress?.current_file" class="merge-progress__file">{{ progress.current_file }}</span>
        <span class="merge-progress__count">
          {{ progress?.current ?? 0 }} / {{ progress?.total ?? items.length }}
        </span>
      </div>
    </div>

    <!-- 完成弹窗 -->
    <el-dialog
      v-model="resultOpen"
      :title="t('合并完成')"
      width="520px"
      :close-on-click-modal="false"
      class="merge-result-dialog"
    >
      <div v-if="result" class="merge-result">
        <div class="merge-result__icon">
          <el-icon :size="30"><CircleCheckFilled /></el-icon>
        </div>
        <h3 class="merge-result__title">{{ t('PDF 已生成') }}</h3>
        <p class="merge-result__path" :title="result.output_path">{{ result.output_path }}</p>
        <div class="merge-result__stats">
          <span><b>{{ result.page_count }}</b>{{ t('页') }}</span>
          <span><b>{{ formatFileSize(result.file_size) }}</b>{{ t('文件大小') }}</span>
          <span><b>{{ result.source_count }}</b>{{ t('个文件') }}</span>
          <span><b>{{ (result.elapsed_ms / 1000).toFixed(2) }}s</b>{{ t('耗时') }}</span>
        </div>
      </div>

      <template #footer>
        <div class="merge-result__actions">
          <el-button :icon="CopyDocument" :loading="copying" @click="copyResult">
            {{ copied ? t('已复制') : t('复制该 PDF') }}
          </el-button>
          <el-button :icon="FolderOpened" @click="openResultFolder">
            {{ t('打开所在文件夹') }}
          </el-button>
          <el-button type="primary" :icon="RefreshLeft" @click="continueMerging">
            {{ t('继续合并') }}
          </el-button>
        </div>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import {
  computed,
  onActivated,
  onBeforeUnmount,
  onDeactivated,
  onMounted,
  ref,
  type Component
} from 'vue'
import { ElMessage } from 'element-plus'
import { open, save } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import {
  ArrowDown,
  ArrowUp,
  CircleCheckFilled,
  Close,
  CopyDocument,
  Delete,
  Document,
  DocumentAdd,
  DocumentCopy,
  FolderOpened,
  Loading,
  MagicStick,
  Picture,
  Rank,
  RefreshLeft,
  Tickets
} from '@element-plus/icons-vue'

import { useSettings } from '@core/hooks/useSettings'
import { hasTauriRuntime } from '@core/utils/tauri'
import { formatBytes as formatFileSize } from '@core/utils/format'
import { openParentDir } from '@core/api/common'
import {
  MERGE_EXTENSIONS,
  analyzeMergeInputs,
  copyPdfFile,
  getOfficeConverterStatus,
  mergePdfDocuments,
  type MergeInputInfo,
  type MergeInputKind,
  type MergeProgress,
  type MergeResult,
  type OfficeConverterStatus
} from './api/pdf-merge'

const { t } = useSettings()

/* ------------------------------------------------------------------ */
/* 状态                                                                */
/* ------------------------------------------------------------------ */

interface MergeListItem extends MergeInputInfo {
  kind: MergeInputKind
  uid: string
  pageSelection: string
}

type SortMode = 'custom' | 'pages_asc' | 'pages_desc' | 'mtime_asc' | 'mtime_desc'

const sortOptions: { value: SortMode; label: string }[] = [
  { value: 'custom', label: '自定义顺序' },
  { value: 'pages_asc', label: '页数：从少到多' },
  { value: 'pages_desc', label: '页数：从多到少' },
  { value: 'mtime_asc', label: '修改时间：从旧到新' },
  { value: 'mtime_desc', label: '修改时间：从新到旧' }
]

const items = ref<MergeListItem[]>([])
const sortMode = ref<SortMode>('custom')
const importing = ref(false)
const merging = ref(false)
const copying = ref(false)
const copied = ref(false)
const importErrors = ref<string[]>([])
const converter = ref<OfficeConverterStatus | null>(null)
const progress = ref<MergeProgress | null>(null)
const result = ref<MergeResult | null>(null)
const resultOpen = ref(false)

const dragIndex = ref<number | null>(null)
const dropIndex = ref<number | null>(null)
const isActive = ref(true)

let uidSeq = 0
const nextUid = () => `merge_${Date.now()}_${++uidSeq}`

/* ------------------------------------------------------------------ */
/* 计算属性                                                            */
/* ------------------------------------------------------------------ */

const converterLabel = computed(() => {
  if (!converter.value) return t('正在检测文档转换器…')
  if (!converter.value.available) return t('未检测到 Word / LibreOffice')
  if (converter.value.libreoffice && converter.value.word) {
    return t('文档转换：LibreOffice / Word')
  }
  if (converter.value.libreoffice) return t('文档转换：LibreOffice')
  return t('文档转换：Microsoft Word')
})

const converterTooltip = computed(() =>
  converter.value?.available
    ? t('DOC/DOCX 会先转换为 PDF，再按页码挑选页面合并')
    : t('安装 LibreOffice 或 Microsoft Word 后即可合并 DOC/DOCX；PDF 与图片不受影响')
)

const totalPages = computed(() =>
  items.value.reduce((sum, item) => {
    if (item.kind !== 'doc') return sum + item.page_count
    const info = selectionInfo(item)
    if (info.error) return sum
    return sum + (info.pages ? info.pages.length : item.page_count)
  }, 0)
)

const canMerge = computed(() => {
  if (!items.value.length || importing.value || merging.value) return false
  return items.value.every((item) => item.kind !== 'doc' || !selectionInfo(item).error)
})

const progressPercent = computed(() => {
  const current = progress.value?.current ?? 0
  const total = progress.value?.total ?? items.value.length
  if (!total) return 0
  return Math.min(100, Math.round((current / total) * 100))
})

const progressPhaseLabel = computed(() => {
  const phase = progress.value?.phase
  const labels: Record<string, string> = {
    pdf: '读取 PDF',
    image: '渲染图片',
    convert: '转换文档',
    done: '写入 PDF'
  }
  return t(labels[phase ?? ''] ?? '准备中')
})

/* ------------------------------------------------------------------ */
/* 页码解析（与后端保持一致，用于即时校验）                            */
/* ------------------------------------------------------------------ */

interface SelectionInfo {
  pages: number[] | null
  error: string | null
  label: string
  custom: boolean
}

function parseSelection(
  raw: string,
  total: number
): { pages: number[] | null; error: string | null } {
  const text = raw.trim()
  if (!text || text === '*' || text === '全部' || text.toLowerCase() === 'all') {
    return { pages: null, error: null }
  }
  if (total <= 0) return { pages: [], error: t('文档没有页面') }

  const normalized = text
    .replace(/，/g, ',')
    .replace(/、/g, ',')
    .replace(/；/g, ';')
    .replace(/[–—～~]/g, '-')

  const pages = new Set<number>()
  for (const rawToken of normalized.split(/[\s,;]+/)) {
    const token = rawToken.trim()
    if (!token) continue
    const match = /^(\d+)(?:\s*-\s*(\d+))?$/.exec(token)
    if (!match) return { pages: [], error: t('格式应为 1,5,7 或 1-3,5') }
    const start = Number(match[1])
    const end = Number(match[2] ?? match[1])
    if (start < 1 || end < 1) return { pages: [], error: t('页码从 1 开始') }
    if (start > end) return { pages: [], error: t('起始页不能大于结束页') }
    if (end > total) return { pages: [], error: t('页码 {n} 超出总页数 {total}', { n: end, total }) }
    for (let page = start; page <= end; page += 1) pages.add(page)
  }

  if (!pages.size) return { pages: null, error: null }
  return { pages: [...pages].sort((a, b) => a - b), error: null }
}

function selectionInfo(item: MergeListItem): SelectionInfo {
  const parsed = parseSelection(item.pageSelection, item.page_count)
  if (parsed.error) {
    return { pages: [], error: parsed.error, label: parsed.error, custom: false }
  }
  if (parsed.pages === null) {
    return {
      pages: null,
      error: null,
      label: t('全部 {n} 页', { n: item.page_count }),
      custom: false
    }
  }
  return {
    pages: parsed.pages,
    error: null,
    label: t('已选 {n} / {total} 页', { n: parsed.pages.length, total: item.page_count }),
    custom: true
  }
}

/* ------------------------------------------------------------------ */
/* 展示辅助                                                            */
/* ------------------------------------------------------------------ */

function kindIcon(kind: MergeInputKind): Component {
  if (kind === 'pdf') return Document
  if (kind === 'image') return Picture
  return Tickets
}

function kindLabel(kind: MergeInputKind): string {
  if (kind === 'pdf') return 'PDF'
  if (kind === 'image') return t('图片')
  return t('Word')
}

function formatModified(ms: number): string {
  if (!ms) return '—'
  const date = new Date(ms)
  if (Number.isNaN(date.getTime())) return '—'
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

const pathKey = (path: string) => path.replace(/\//g, '\\').toLowerCase()

/* ------------------------------------------------------------------ */
/* 导入                                                                */
/* ------------------------------------------------------------------ */

async function addFiles() {
  if (importing.value || merging.value) return
  try {
    const selected = await open({
      multiple: true,
      title: t('选择要合并的文件'),
      filters: [
        { name: t('支持的文件'), extensions: [...MERGE_EXTENSIONS] },
        { name: 'PDF', extensions: ['pdf'] },
        { name: t('图片'), extensions: ['jpg', 'jpeg', 'png', 'webp', 'tif', 'tiff', 'bmp', 'gif'] },
        { name: 'Word', extensions: ['doc', 'docx'] }
      ]
    })
    if (!selected) return
    const paths = Array.isArray(selected) ? selected : [selected]
    await importPaths(paths)
  } catch (error: any) {
    ElMessage.error(`${t('选择文件失败')}：${String(error?.message || error)}`)
  }
}

async function importPaths(paths: string[]) {
  if (!paths.length) return

  const known = new Set(items.value.map((item) => pathKey(item.path)))
  const unique: string[] = []
  for (const path of paths) {
    if (!path) continue
    const key = pathKey(path)
    if (known.has(key)) continue
    known.add(key)
    unique.push(path)
  }
  if (!unique.length) {
    ElMessage.info(t('这些文件已经在列表中'))
    return
  }

  importing.value = true
  importErrors.value = []
  try {
    const infos = await analyzeMergeInputs(unique)
    const failures: string[] = []
    let added = 0

    for (const info of infos) {
      if (info.error || info.kind === 'unknown') {
        failures.push(`${info.name}：${info.error ?? t('无法识别的文件')}`)
        continue
      }
      items.value.push({
        path: info.path,
        name: info.name,
        kind: info.kind as MergeInputKind,
        extension: info.extension,
        page_count: info.page_count,
        file_size: info.file_size,
        modified_ms: info.modified_ms,
        width: info.width,
        height: info.height,
        uid: nextUid(),
        pageSelection: ''
      })
      added += 1
    }

    if (failures.length) importErrors.value = failures
    if (added && sortMode.value !== 'custom') applySort()
    if (added) ElMessage.success(t('已添加 {n} 个文件', { n: added }))
  } catch (error: any) {
    ElMessage.error(`${t('导入失败')}：${String(error?.message || error)}`)
  } finally {
    importing.value = false
  }
}

/* ------------------------------------------------------------------ */
/* 排序 / 拖拽                                                         */
/* ------------------------------------------------------------------ */

function applySort() {
  const mode = sortMode.value
  if (mode === 'custom' || items.value.length < 2) return
  const descending = mode.endsWith('_desc')
  const byPages = mode.startsWith('pages')
  items.value = [...items.value].sort((a, b) => {
    const left = byPages ? a.page_count : a.modified_ms
    const right = byPages ? b.page_count : b.modified_ms
    if (left === right) return 0
    return (left < right ? -1 : 1) * (descending ? -1 : 1)
  })
}

function onDragStart(index: number, event: DragEvent) {
  dragIndex.value = index
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', String(index))
  }
}

function onDragOver(index: number) {
  if (dragIndex.value === null) return
  dropIndex.value = index
}

function onDragLeave(index: number) {
  if (dropIndex.value === index) dropIndex.value = null
}

function onDrop(index: number, event: DragEvent) {
  const from = dragIndex.value
  // 系统文件拖放（dragIndex 为空）交给根节点 onDomDrop 处理，不要吃掉事件。
  if (from === null) return
  event.stopPropagation()
  resetDrag()
  if (from === index) return

  const next = [...items.value]
  const [moved] = next.splice(from, 1)
  if (!moved) return
  next.splice(index, 0, moved)
  items.value = next
  sortMode.value = 'custom'
}

function resetDrag() {
  dragIndex.value = null
  dropIndex.value = null
}

function moveItem(index: number, delta: number) {
  const target = index + delta
  if (target < 0 || target >= items.value.length) return
  const next = [...items.value]
  const [moved] = next.splice(index, 1)
  if (!moved) return
  next.splice(target, 0, moved)
  items.value = next
  sortMode.value = 'custom'
}

function removeItem(index: number) {
  items.value.splice(index, 1)
}

function clearAll() {
  items.value = []
  importErrors.value = []
  sortMode.value = 'custom'
}

/** DOM 拖放兜底：Tauri 桌面端主要走 onDragDropEvent，浏览器预览用 File.path。 */
function onDomDrop(event: DragEvent) {
  if (dragIndex.value !== null) {
    resetDrag()
    return
  }
  const files = Array.from(event.dataTransfer?.files ?? [])
  if (!files.length) return
  const paths = files
    .map((file) => (file as File & { path?: string }).path)
    .filter((path): path is string => Boolean(path))
  if (paths.length) {
    void importPaths(paths)
  } else {
    ElMessage.warning(t('请使用桌面应用拖入文件（浏览器预览无法读取本地路径）'))
  }
}

/* ------------------------------------------------------------------ */
/* 合并                                                                */
/* ------------------------------------------------------------------ */

function defaultOutputPath(): string {
  const first = items.value[0]
  if (!first) return '合并结果.pdf'
  const separator = first.path.includes('\\') ? '\\' : '/'
  const cut = first.path.lastIndexOf(separator)
  const directory = cut > 0 ? first.path.slice(0, cut) : ''
  const base = first.name.replace(/\.[^.]+$/, '') || '合并结果'
  return directory ? `${directory}${separator}${base}_合并.pdf` : `${base}_合并.pdf`
}

async function mergeNow() {
  if (!canMerge.value) return

  const invalid = items.value.find(
    (item) => item.kind === 'doc' && selectionInfo(item).error
  )
  if (invalid) {
    ElMessage.error(`${invalid.name}：${selectionInfo(invalid).error}`)
    return
  }

  let target: string | null = null
  try {
    target = await save({
      title: t('保存合并后的 PDF'),
      defaultPath: defaultOutputPath(),
      filters: [{ name: 'PDF', extensions: ['pdf'] }]
    })
  } catch (error: any) {
    ElMessage.error(`${t('选择输出路径失败')}：${String(error?.message || error)}`)
    return
  }
  if (!target) return

  merging.value = true
  copied.value = false
  progress.value = {
    current: 0,
    total: items.value.length,
    current_file: '',
    phase: 'pdf'
  }

  try {
    const response = await mergePdfDocuments(
      items.value.map((item) => ({
        path: item.path,
        page_selection: item.kind === 'doc' ? item.pageSelection.trim() || null : null
      })),
      target
    )
    result.value = response
    resultOpen.value = true
    ElMessage.success(t('合并完成'))
  } catch (error: any) {
    ElMessage.error(`${t('合并失败')}：${String(error?.message || error)}`)
  } finally {
    merging.value = false
  }
}

async function copyResult() {
  if (!result.value || copying.value) return
  copying.value = true
  try {
    await copyPdfFile(result.value.output_path)
    copied.value = true
    ElMessage.success(t('已复制 PDF 文件'))
  } catch (error: any) {
    ElMessage.error(`${t('复制失败')}：${String(error?.message || error)}`)
  } finally {
    copying.value = false
  }
}

async function openResultFolder() {
  if (!result.value) return
  try {
    await openParentDir(result.value.output_path)
  } catch (error: any) {
    ElMessage.error(`${t('打开失败')}：${String(error?.message || error)}`)
  }
}

function continueMerging() {
  resultOpen.value = false
  result.value = null
  progress.value = null
  items.value = []
  importErrors.value = []
  sortMode.value = 'custom'
  copied.value = false
}

/* ------------------------------------------------------------------ */
/* 生命周期                                                            */
/* ------------------------------------------------------------------ */

let unlistenProgress: (() => void) | null = null
let unlistenDrop: (() => void) | null = null

onMounted(async () => {
  isActive.value = true
  if (!hasTauriRuntime()) {
    converter.value = { libreoffice: null, word: false, available: false }
    return
  }

  unlistenProgress = await listen<MergeProgress>('pdf_merge_progress', (event) => {
    progress.value = event.payload
  })

  try {
    unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
      if (!isActive.value) return
      if (event.payload.type === 'drop' && event.payload.paths?.length) {
        void importPaths(event.payload.paths)
      }
    })
  } catch {
    // 浏览器预览没有 Tauri 拖放事件，DOM 兜底仍然可用。
  }

  try {
    converter.value = await getOfficeConverterStatus()
  } catch {
    converter.value = { libreoffice: null, word: false, available: false }
  }
})

onActivated(() => {
  isActive.value = true
})

onDeactivated(() => {
  isActive.value = false
})

onBeforeUnmount(() => {
  unlistenProgress?.()
  unlistenDrop?.()
  unlistenProgress = null
  unlistenDrop = null
})
</script>

<style scoped>
.pdf-merge-tool {
  display: flex;
  flex-direction: column;
  gap: 10px;
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

/* ============ Toolbar ============ */
.merge-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  flex-shrink: 0;
  padding: 9px 12px;
  border-radius: var(--radius-md);
  background: var(--surface-block);
}
.merge-sort {
  width: 176px;
}
.merge-count {
  font-size: 12px;
  color: var(--text-muted);
  white-space: nowrap;
}
.merge-toolbar__spacer {
  flex: 1;
  min-width: 8px;
}

.converter-hint {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 9px;
  border-radius: var(--radius-pill);
  background: var(--surface-inset);
  color: var(--text-muted);
  font-size: 11px;
  white-space: nowrap;
  cursor: default;
}
.converter-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--success, #35a26b);
  flex-shrink: 0;
}
.converter-hint.is-warn {
  color: var(--warning, #c98212);
}
.converter-hint.is-warn .converter-dot {
  background: var(--warning, #c98212);
}

/* ============ Import states ============ */
.merge-importing {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  padding: 8px 12px;
  border-radius: var(--radius-md);
  background: var(--accent-light);
  color: var(--accent-text);
  font-size: 12px;
}

.merge-import-errors {
  flex-shrink: 0;
  padding: 10px 12px;
  border-radius: var(--radius-md);
  background: var(--warning-light, rgba(217, 140, 20, 0.1));
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.6;
}
.merge-import-errors__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-weight: 600;
  color: var(--warning, #c98212);
}
.merge-import-errors__close {
  border: none;
  background: transparent;
  color: inherit;
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  padding: 0 2px;
}
.merge-import-errors__item {
  margin-top: 3px;
  word-break: break-all;
}

/* ============ Body / list ============ */
.merge-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 9px;
  overflow-y: auto;
  padding: 1px 3px 2px 1px;
}

/* ============ Empty state ============ */
.merge-empty {
  flex: 1;
  min-height: 340px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 36px 24px;
  text-align: center;
  border: 1px dashed var(--border-secondary);
  border-radius: var(--radius-card);
  background: var(--surface-block);
}
.merge-empty__icon {
  display: grid;
  place-items: center;
  width: 68px;
  height: 68px;
  border-radius: 20px;
  background: var(--accent-light);
  color: var(--accent-text);
  margin-bottom: 2px;
}
.merge-empty__title {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}
.merge-empty__desc {
  margin: 0;
  max-width: 520px;
  font-size: 12.5px;
  line-height: 1.6;
  color: var(--text-secondary);
}
.merge-empty__tip {
  margin: 4px 0 0;
  font-size: 11.5px;
  color: var(--text-muted);
}

/* ============ File card ============ */
.merge-card {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
  padding: 11px 12px;
  border-radius: var(--radius-md);
  background: var(--color-card);
  box-shadow: var(--card-shadow);
  transition: box-shadow var(--duration-fast) var(--ease-standard),
              opacity var(--duration-fast) var(--ease-standard),
              transform var(--duration-fast) var(--ease-standard);
}
.merge-card:hover {
  box-shadow: var(--home-card-hover-shadow, var(--card-shadow));
}
.merge-card.is-dragging {
  opacity: 0.45;
}
.merge-card.is-drop-target {
  box-shadow: inset 0 0 0 1.5px var(--accent), var(--card-shadow);
}

.drag-handle {
  display: grid;
  place-items: center;
  width: 22px;
  height: 30px;
  flex-shrink: 0;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-muted);
  cursor: grab;
}
.drag-handle:active {
  cursor: grabbing;
}
.drag-handle:hover {
  background: var(--bg-hover);
  color: var(--text-secondary);
}

.merge-card__index {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  flex-shrink: 0;
  border-radius: 50%;
  background: var(--surface-inset);
  color: var(--text-secondary);
  font-size: 11.5px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.merge-card__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.merge-card__head {
  display: flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
}
.kind-icon {
  flex-shrink: 0;
}
.kind-icon--pdf {
  color: #d24a4a;
}
.kind-icon--image {
  color: #2f9e6f;
}
.kind-icon--doc {
  color: #3b7dd8;
}
.merge-card__name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  font-weight: 550;
  color: var(--text-primary);
}
.kind-chip {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  height: 18px;
  padding: 0 7px;
  border-radius: var(--radius-pill);
  font-size: 10.5px;
  font-weight: 500;
}
.kind-chip--pdf {
  background: rgba(210, 74, 74, 0.12);
  color: #c23f3f;
}
.kind-chip--image {
  background: rgba(47, 158, 111, 0.13);
  color: #27845c;
}
.kind-chip--doc {
  background: rgba(59, 125, 216, 0.13);
  color: #326fc4;
}

.merge-card__meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px 12px;
  font-size: 11.5px;
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}

/* ============ Word page picker ============ */
.page-picker {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 7px;
  margin-top: 4px;
}
.page-picker__label {
  font-size: 11.5px;
  color: var(--text-secondary);
  white-space: nowrap;
}
.page-picker__field {
  width: 236px;
  max-width: 100%;
}
.page-picker__field.is-invalid :deep(.el-input__wrapper) {
  box-shadow: 0 0 0 1px var(--el-color-danger) inset;
}
.page-picker__status {
  font-size: 11.5px;
  color: var(--text-muted);
  white-space: nowrap;
}
.page-picker__status.is-accent {
  color: var(--accent-text);
}
.page-picker__status.is-invalid {
  color: var(--el-color-danger);
}

/* ============ Row actions ============ */
.merge-card__ops {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}
.merge-op {
  display: grid;
  place-items: center;
  width: 27px;
  height: 27px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition: background var(--duration-fast) ease, color var(--duration-fast) ease;
}
.merge-op:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.merge-op:disabled {
  opacity: 0.3;
  cursor: default;
}
.merge-op.is-danger:hover:not(:disabled) {
  background: var(--danger-light, rgba(220, 70, 70, 0.12));
  color: var(--el-color-danger);
}

/* ============ Add more ============ */
.merge-add-more {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  flex-shrink: 0;
  padding: 10px;
  border: 1px dashed var(--border-secondary);
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: border-color var(--duration-fast) ease, color var(--duration-fast) ease,
              background var(--duration-fast) ease;
}
.merge-add-more:hover:not(:disabled) {
  border-color: var(--accent-text);
  color: var(--accent-text);
  background: var(--accent-light);
}
.merge-add-more:disabled {
  opacity: 0.5;
  cursor: default;
}

/* ============ Progress ============ */
.merge-progress {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border-radius: var(--radius-md);
  background: var(--surface-block);
}
.merge-progress__text {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11.5px;
  color: var(--text-secondary);
  min-width: 0;
}
.merge-progress__file {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-muted);
}
.merge-progress__count {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
  color: var(--text-muted);
}

/* ============ Result dialog ============ */
.merge-result {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  text-align: center;
}
.merge-result__icon {
  display: grid;
  place-items: center;
  width: 58px;
  height: 58px;
  border-radius: 50%;
  background: var(--success-light, rgba(53, 162, 107, 0.12));
  color: var(--success, #35a26b);
}
.merge-result__title {
  margin: 2px 0 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}
.merge-result__path {
  margin: 0;
  max-width: 100%;
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  background: var(--surface-inset);
  color: var(--text-secondary);
  font-size: 11.5px;
  line-height: 1.5;
  word-break: break-all;
}
.merge-result__stats {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 6px 18px;
  margin-top: 6px;
  font-size: 11.5px;
  color: var(--text-muted);
}
.merge-result__stats b {
  color: var(--text-primary);
  font-size: 13px;
  font-weight: 600;
  margin-right: 4px;
  font-variant-numeric: tabular-nums;
}
.merge-result__actions {
  display: flex;
  justify-content: center;
  gap: 8px;
  flex-wrap: wrap;
}

/* ============ Responsive ============ */
@media (max-width: 820px) {
  .merge-toolbar {
    gap: 6px;
  }
  .merge-sort {
    width: 150px;
  }
  .merge-count,
  .converter-hint {
    display: none;
  }
  .merge-card {
    flex-wrap: wrap;
  }
  .merge-card__ops {
    margin-left: auto;
  }
  .page-picker__field {
    width: 100%;
  }
}

@media (prefers-reduced-motion: reduce) {
  .merge-card,
  .merge-op,
  .merge-add-more {
    transition: none !important;
  }
}
</style>
