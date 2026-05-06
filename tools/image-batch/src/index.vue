<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { ElMessage } from 'element-plus'
import { FolderOpened, Upload as UploadIcon, MagicStick, Document } from '@element-plus/icons-vue'
import ImageStatusCard from './components/image-status-card.vue'
import { convertImages, convertToEpub, listImages, readTool3Note } from './api/image-batch'
import { openParentDir } from '@core/api/common'
import { useFileSelect } from '@core/hooks/useFileSelect'
import { useSettings } from '@core/hooks/useSettings'
import { formatBytes } from '@core/utils/format'
import type { ConvertReq, ProblemItem, EpubPageSize, EpubConvertReq } from './types/image'
import { EPUB_PAGE_SIZES } from './types/image'

const { pick } = useFileSelect()
const { t } = useSettings()

const inputDir = ref('')
const outputDir = ref('')
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
    const res = await listImages(inputDir.value, true)
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
        pageSize: epubPageSize.value
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
        losslessMerge: losslessMerge.value
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
  if (unlistenProgress) return
  unlistenProgress = await listen('progress-update', (event) => {
    const payload = event.payload as any
    if (!payload || !payload.stage || !payload.total) return
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
    noteContent.value = await readTool3Note()
  } catch (e: any) {
    ElMessage.error(e?.toString() || t('读取说明失败'))
  } finally {
    noteLoading.value = false
  }
}

onMounted(setupProgress)
onBeforeUnmount(() => {
  unlistenProgress?.()
  unlistenProgress = null
})
</script>

<template>
  <div class="tool3">
    <div class="form-row">
      <label>{{ t('输入目录') }}</label>
      <div class="inline">
        <el-input v-model="inputDir" :placeholder="t('选择包含图片的文件夹')" />
        <el-button :icon="FolderOpened" @click="pickFolder('input')">{{ t('浏览') }}</el-button>
        <el-button :icon="UploadIcon" @click="pastePath">{{ t('粘贴') }}</el-button>
      </div>
    </div>
    <div class="form-row">
      <label>{{ t('输出目录') }}</label>
      <div class="inline">
        <el-input
          v-model="outputDir"
          :placeholder="t('留空默认：同级生成“输入文件夹名_合并.pdf”；填写自定义名自动补 .pdf')"
        />
        <el-button :icon="FolderOpened" @click="pickFolder('output')">{{ t('浏览') }}</el-button>
      </div>
    </div>
    <!-- 输出格式选择 -->
    <div class="form-row">
      <label>{{ t('输出格式') }}</label>
      <el-radio-group v-model="outputFormat" size="small">
        <el-radio label="pdf">PDF</el-radio>
        <el-radio label="epub">EPUB</el-radio>
      </el-radio-group>
    </div>

    <!-- PDF 特有选项 -->
    <template v-if="outputFormat === 'pdf'">
      <div class="form-row">
        <label>{{ t('批大小') }}</label>
        <el-radio-group v-model="batchSize" size="small">
          <el-radio :label="100">100</el-radio>
          <el-radio :label="200">200</el-radio>
          <el-radio :label="300">300</el-radio>
          <el-radio :label="0">{{ t('全部合并') }}</el-radio>
        </el-radio-group>
      </div>
      
      <div class="form-row">
        <label>{{ t('无损合并') }}</label>
        <div class="inline">
          <el-switch
            v-model="losslessMerge"
            :active-text="t('智能质量控制，控制文件大小在原图±5%以内')"
            :inactive-text="t('普通质量合并')"
          />
        </div>
      </div>
    </template>

    <!-- EPUB 特有选项 -->
    <template v-if="outputFormat === 'epub'">
      <div class="form-row">
        <label>{{ t('页面尺寸') }}</label>
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

    <div class="buttons">
      <el-button type="primary" :icon="MagicStick" :loading="listLoading" @click="importList">
        {{ t('导入列表') }}
      </el-button>
      <el-button type="success" :loading="running" @click="runConvert">{{ t('开始转换') }}</el-button>
      <el-button @click="clearList">{{ t('清空列表') }}</el-button>
    </div>

    <ImageStatusCard :progress="progress" :result-info="resultInfo" :problem-log="problemLog" @openOutput="openOutput" />

    <el-table :data="files" height="260" size="small" v-loading="listLoading">
      <el-table-column prop="name" :label="t('文件名')" />
      <el-table-column prop="format" :label="t('格式')" width="80" />
      <el-table-column :label="t('大小')" width="120">
        <template #default="{ row }">{{ formatBytes(row.size) }}</template>
      </el-table-column>
    </el-table>

    <div class="note-link">
      <el-link type="info" :icon="Document" @click="openNote">{{ t('小工具说明') }}</el-link>
    </div>

    <el-dialog v-model="noteVisible" :title="t('工具 3 说明')" width="640px">
      <el-scrollbar height="360px">
        <pre class="note-content" v-loading="noteLoading">{{ noteContent || t('未找到说明内容') }}</pre>
      </el-scrollbar>
    </el-dialog>
  </div>
</template>

<style scoped>
.tool3 {
  display: flex;
  flex-direction: column;
  gap: 12px;
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
.buttons {
  display: flex;
  gap: 10px;
  align-items: center;
}
.note-link {
  align-self: flex-start;
  font-size: 12px;
  color: #666;
}
.note-content {
  white-space: pre-wrap;
  font-family: Consolas, 'SFMono-Regular', monospace;
  line-height: 1.5;
  margin: 0;
}
.size-desc {
  font-size: 11px;
  color: #888;
}
</style>
