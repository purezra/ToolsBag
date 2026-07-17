<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { ElMessage } from 'element-plus'
import { FolderOpened, Upload as UploadIcon, MagicStick, Document } from '@element-plus/icons-vue'
import ImageStatusCard from './components/image-status-card.vue'
import { convertImages, convertToEpub, listImages, readImageBatchNote } from './api/image-batch'
import { openParentDir } from '@core/api/common'
import { useFileSelect } from '@core/hooks/useFileSelect'
import { useSettings } from '@core/hooks/useSettings'
import { formatBytes } from '@core/utils/format'
import { hasTauriRuntime } from '@core/utils/tauri'
import type { ConvertReq, ProblemItem, EpubPageSize, EpubConvertReq } from './types/image'
import { EPUB_PAGE_SIZES } from './types/image'

const { pick } = useFileSelect()
const { t } = useSettings()

const inputDir = ref('')
const outputDir = ref('')
const recursive = ref(true) // 递归扫描子目录（导入列表与转换保持一致）
const batchSize = ref<number>(100) // 0 表示全部合并
const losslessMerge = ref(true) // 无损合并开关
const outputFormat = ref<'pdf' | 'epub'>('pdf') // 输出格式
const epubPageSize = ref<EpubPageSize>('ipadpro') // EPUB 页面尺寸
const running = ref(false)
const listLoading = ref(false)
const files = ref<{ name: string; format: string; size: number }[]>([])
const problemLog = ref('')
const progress = ref<{ stage: string; percent: number; message: string } | null>(null)
const resultInfo = ref<{
  path: string
  inputSize: string
  outputSize: string
  deltaText: string
  increase: boolean
  pageCount?: number
  pageSize?: string
} | null>(null)
const noteVisible = ref(false)
const noteContent = ref('')
const noteLoading = ref(false)
let unlistenProgress: (() => void) | null = null

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

const importList = async () => {
  if (!inputDir.value) {
    ElMessage.warning(t('请选择输入目录'))
    return
  }
  listLoading.value = true
  try {
    const res = await listImages(inputDir.value, recursive.value)
    files.value = res.map((f) => ({ name: f.name, format: f.format, size: f.size }))
    showImportStats()
  } catch (e: any) {
    ElMessage.error(e?.toString() || t('导入失败'))
  } finally {
    listLoading.value = false
  }
}

const buildDelta = (orig: number, out: number) => {
  const diff = out - orig
  const pct = orig > 0 ? (diff / orig) * 100 : 0
  const sign = diff >= 0 ? '+' : '-'
  const absDiff = Math.abs(diff)
  const increase = diff >= 0
  return {
    text: `${sign}${formatBytes(absDiff)} ${increase ? 'up' : 'down'} ${Math.abs(pct).toFixed(2)}%`,
    increase
  }
}

const runConvert = async () => {
  if (!inputDir.value) {
    ElMessage.warning(t('请选择输入目录'))
    return
  }
  running.value = true
  resultInfo.value = null
  try {
    if (outputFormat.value === 'epub') {
      // EPUB 转换
      const res = await convertToEpub({
        inputDir: inputDir.value,
        outputDir: outputDir.value || null,
        pageSize: epubPageSize.value,
        recursive: recursive.value
      } as EpubConvertReq)
      const delta = buildDelta(res.originalBytes, res.outputBytes)
      resultInfo.value = {
        path: res.outputPath,
        inputSize: formatBytes(res.originalBytes),
        outputSize: formatBytes(res.outputBytes),
        deltaText: delta.text,
        increase: delta.increase,
        pageCount: res.pageCount,
        pageSize: res.pageSize
      }
      if (res.problems && res.problems.length) {
        problemLog.value = res.problems.map((p: ProblemItem) => `${p.path}: ${p.reasons.join(',')}`).join('\n')
        ElMessage.warning(`${t('EPUB 转换完成有问题')} (${res.problems.length})`)
      } else {
        problemLog.value = t('暂无问题')
        ElMessage.success(`${t('EPUB 转换完成')}，共 ${res.pageCount} 页，尺寸: ${res.pageSize}`)
      }
    } else {
      // PDF 转换
      const res = await convertImages({
        inputDir: inputDir.value,
        outputDir: outputDir.value || null,
        batchSize: batchSize.value || null, // 0 转为 null 表示全部合并
        losslessMerge: losslessMerge.value,
        recursive: recursive.value
      } as ConvertReq)
      const delta = buildDelta(res.originalBytes, res.outputBytes)
      resultInfo.value = {
        path: res.outputPath,
        inputSize: formatBytes(res.originalBytes),
        outputSize: formatBytes(res.outputBytes),
        deltaText: delta.text,
        increase: delta.increase
      }
      if (res.problems && res.problems.length) {
        problemLog.value = res.problems.map((p: ProblemItem) => `${p.path}: ${p.reasons.join(',')}`).join('\n')
        ElMessage.warning(`${t('转换完成有问题')} (${res.problems.length})`)
      } else {
        problemLog.value = t('暂无问题')
        ElMessage.success(t('转换完成'))
      }
    }
  } catch (e: any) {
    ElMessage.error(e?.toString() || t('转换失败'))
  } finally {
    running.value = false
  }
}

const setupProgress = async () => {
  // 纯浏览器（npm run dev 无 Tauri 壳）下 __TAURI_INTERNALS__ 缺失，
  // listen() 会抛 "Cannot read properties of undefined (reading 'transformCallback')"。
  if (!hasTauriRuntime()) return
  if (unlistenProgress) return
  unlistenProgress = await listen('progress-update', (event) => {
    const payload = event.payload as any
    // 只处理本工具的进度事件，避免同进程其他工具（如 media_import）的进度串扰。
    if (!payload || (payload.stage !== 'convert3' && payload.stage !== 'epub_convert')) return
    if (!payload.total) return
    const percent = Math.floor(((payload.current || 0) / (payload.total || 1)) * 100)
    progress.value = {
      stage: payload.stage,
      percent,
      message: payload.message || ''
    }
  })
}

const openOutput = async (path: string) => {
  if (!path) return
  await openParentDir(path).catch(() => {
    ElMessage.error(t('无法打开输出目录'))
  })
}

// 折叠标题里的设置摘要，一眼看当前关键选项
const outputSettingsSummary = computed(() => {
  if (outputFormat.value === 'epub') {
    const size = EPUB_PAGE_SIZES.find(s => s.value === epubPageSize.value)
    return `EPUB · ${size?.label ?? epubPageSize.value}`
  }
  const batch = batchSize.value === 0 ? t('全部合并') : batchSize.value
  return `PDF · ${batch} · ${losslessMerge.value ? t('无损') : t('普通')}`
})

const showImportStats = () => {
  const total = files.value.length
  const lower = files.value.map((f) => f.format.toLowerCase())
  const jpg = lower.filter((f) => f === 'jpg').length
  const jpeg = lower.filter((f) => f === 'jpeg').length
  const png = lower.filter((f) => f === 'png').length
  if (total > 0) {
    ElMessage.info(
      `${t('共计图片统计')}: ${total} | jpg ${jpg}, jpeg ${jpeg}, png ${png}`
    )
  }
}

const clearList = () => {
  files.value = []
  problemLog.value = ''
  progress.value = null
  resultInfo.value = null
  ElMessage.success(t('列表已清空'))
}

const openNote = async () => {
  noteVisible.value = true
  if (noteContent.value) return
  noteLoading.value = true
  try {
    noteContent.value = await readImageBatchNote()
  } catch (e: any) {
    ElMessage.error(e?.toString() || t('读取说明失败'))
  } finally {
    noteLoading.value = false
  }
}

let isMounted = false
onMounted(async () => {
  isMounted = true
  await setupProgress()
  // 若在 listen 解析前组件已卸载，立即解除监听避免泄漏。
  if (!isMounted && unlistenProgress) {
    unlistenProgress()
    unlistenProgress = null
  }
})
onBeforeUnmount(() => {
  isMounted = false
  unlistenProgress?.()
  unlistenProgress = null
})
</script>

<template>
  <div class="image-batch-tool">
    <!-- 顶部操作栏：路径输入 + 主操作 -->
    <div class="tb-toolbar image-batch-toolbar">
      <el-input v-model="inputDir" :placeholder="t('选择包含图片的文件夹')" class="tb-toolbar__grow">
        <template #append>
          <el-button :icon="FolderOpened" @click="pickFolder('input')">{{ t('浏览') }}</el-button>
        </template>
      </el-input>
      <el-button :icon="UploadIcon" @click="pastePath">{{ t('粘贴') }}</el-button>
      <el-checkbox v-model="recursive">{{ t('递归') }}</el-checkbox>
      <el-button type="primary" :icon="MagicStick" :loading="listLoading" @click="importList">
        {{ t('导入列表') }}
      </el-button>
      <el-button type="success" :loading="running" @click="runConvert">{{ t('开始转换') }}</el-button>
      <el-button @click="clearList">{{ t('清空') }}</el-button>
      <span class="tb-toolbar__hint">
        <el-link type="info" :icon="Document" @click="openNote">{{ t('说明') }}</el-link>
      </span>
    </div>

    <!-- 输出设置（可折叠，默认收起） -->
    <div class="tb-collapse">
      <el-collapse>
        <el-collapse-item>
          <template #title>
            {{ t('输出设置') }}
            <span class="tb-collapse-summary">{{ outputSettingsSummary }}</span>
          </template>
          <div class="tb-form-row">
            <label class="tb-field-label">{{ t('输出目录') }}</label>
            <div class="tb-inline">
              <el-input
                v-model="outputDir"
                :placeholder="t('留空默认：同级生成 输入文件夹名_合并.pdf；填写自定义名自动补 .pdf')"
              />
              <el-button :icon="FolderOpened" @click="pickFolder('output')">{{ t('浏览') }}</el-button>
            </div>
          </div>
          <div class="tb-form-row">
            <label class="tb-field-label">{{ t('输出格式') }}</label>
            <el-radio-group v-model="outputFormat" size="small">
              <el-radio label="pdf">PDF</el-radio>
              <el-radio label="epub">EPUB</el-radio>
            </el-radio-group>
          </div>

          <template v-if="outputFormat === 'pdf'">
            <div class="tb-form-row">
              <label class="tb-field-label">{{ t('批大小') }}</label>
              <el-radio-group v-model="batchSize" size="small">
                <el-radio :label="100">100</el-radio>
                <el-radio :label="200">200</el-radio>
                <el-radio :label="300">300</el-radio>
                <el-radio :label="0">{{ t('全部合并') }}</el-radio>
              </el-radio-group>
            </div>
            <div class="tb-form-row">
              <label class="tb-field-label">{{ t('无损合并') }}</label>
              <div class="tb-inline">
                <el-switch
                  v-model="losslessMerge"
                  :active-text="t('智能质量控制，控制文件大小在原图±5%以内')"
                  :inactive-text="t('普通质量合并')"
                />
              </div>
            </div>
          </template>

          <template v-if="outputFormat === 'epub'">
            <div class="tb-form-row">
              <label class="tb-field-label">{{ t('页面尺寸') }}</label>
              <el-radio-group v-model="epubPageSize" size="small">
                <el-radio
                  v-for="size in EPUB_PAGE_SIZES"
                  :key="size.value"
                  :label="size.value"
                >
                  {{ size.label }} <span class="size-desc">({{ size.desc }})</span>
                </el-radio>
              </el-radio-group>
            </div>
          </template>
        </el-collapse-item>
      </el-collapse>
    </div>

    <!-- 进度 / 结果 / 问题 -->
    <ImageStatusCard :progress="progress" :result-info="resultInfo" :problem-log="problemLog" @openOutput="openOutput" />

    <!-- 文件列表 -->
    <div class="image-batch-body">
      <div v-if="files.length > 0" class="tb-section file-list-section">
        <div class="tb-section-title">
          {{ t('文件列表') }}
          <el-tag size="small" type="info" class="file-count-tag">{{ files.length }}</el-tag>
        </div>
        <el-table :data="files" size="small" height="100%" v-loading="listLoading">
          <el-table-column prop="name" :label="t('文件名')" />
          <el-table-column prop="format" :label="t('格式')" width="80">
            <template #default="{ row }">
              <el-tag size="small" type="info">.{{ row.format }}</el-tag>
            </template>
          </el-table-column>
          <el-table-column :label="t('大小')" width="120">
            <template #default="{ row }">{{ formatBytes(row.size) }}</template>
          </el-table-column>
        </el-table>
      </div>

      <div v-else-if="!listLoading" class="tb-empty">
        <div class="tb-empty-icon">&#128193;</div>
        <div class="tb-empty-text">{{ t('暂无文件') }}</div>
        <div class="tb-empty-hint">{{ t('点击上方导入列表开始') }}</div>
      </div>
    </div>

    <el-dialog v-model="noteVisible" :title="t('图片批处理说明')" width="640px">
      <el-scrollbar height="360px">
        <pre class="note-content" v-loading="noteLoading">{{ noteContent || t('未找到说明内容') }}</pre>
      </el-scrollbar>
    </el-dialog>
  </div>
</template>

<style scoped>
.image-batch-tool {
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: 100%;
  overflow: hidden;
  min-height: 0;
}
.image-batch-toolbar {
  flex-shrink: 0;
}
.image-batch-body {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.file-list-section {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.file-list-section :deep(.el-table) {
  flex: 1;
  min-height: 0;
}
.file-count-tag {
  margin-left: 6px;
}
.note-content {
  white-space: pre-wrap;
  font-family: var(--font-mono);
  line-height: 1.5;
  margin: 0;
}
.size-desc {
  font-size: 11px;
  color: var(--text-muted);
}
</style>
