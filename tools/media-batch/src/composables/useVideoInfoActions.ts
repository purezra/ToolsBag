import { reactive, ref, shallowRef, computed, nextTick, type Ref, type ComputedRef } from 'vue'
import { ElMessage } from 'element-plus'
import { save, open as openDialog } from '@tauri-apps/plugin-dialog'
import { useFileSelect } from '@core/hooks/useFileSelect'
import { formatBytes, formatDuration } from '@core/utils/format'
import { writeBinaryExportFile, writeTextExportFile } from '@core/api/common'
import { importDetailedVideoInfo, recordMediaPerformance, getVideoRawXml, getVideoCompleteInfo, getVideoXmlJson, getVideoXmlMarkdown } from '../api/media-batch'
import { saveVideoRecords, loadVideoRecords, deleteVideoRecords, getRecordCount, rowToVideoInfoItem, type VideoRecordRow } from '../api/video-db'
import type { VideoInfoImportResponse, VideoInfoItem, HealthIssue } from '../types/media'
import type { ColumnDef } from './useVideoInfoColumns'

export type ExportFormat = 'xlsx' | 'csv' | 'markdown' | 'html' | 'json' | 'txt' | 'xml2json' | 'xml2md'
export type ImportTiming = { metadataMs: number; displayMs: number; totalMs: number; cached: boolean }

// ==================== 导出辅助 ====================

const csvEscape = (v: string | number) => {
  const s = String(v)
  return s.includes(',') || s.includes('"') || s.includes('\n')
    ? `"${s.replace(/"/g, '""')}"` : s
}

const escapeHtml = (value: string | number) =>
  String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')

// ==================== Composable ====================

export function useVideoInfoActions(
  items: Ref<VideoInfoItem[]>,
  deps: {
    importSummary: Ref<string>
    expandedIds: Ref<Set<number>>
    searchQuery: Ref<string>
    level: Ref<string>
    recursive: Ref<boolean>
    emit: (e: 'addToRename', items: VideoInfoItem[]) => void
    /** 来自 columns composable 的导出数据 */
    preFilterTableData: ComputedRef<any[]>
    flatTableData: ComputedRef<any[]>
    visibleColumns: ComputedRef<ColumnDef[]>
    /** 来自 health composable */
    healthSummary: ComputedRef<{ danger: number; warning: number; info: number; totalIssues: number; cleanFiles: number }>
    /** i18n */
    t: (key: string) => string
  }
) {
  const { importSummary, expandedIds, searchQuery, recursive, emit, preFilterTableData, flatTableData, visibleColumns, healthSummary, t } = deps

  const { pick } = useFileSelect()

  // ==================== 导入 ====================
  const importing = ref(false)
  const importProgress = reactive({ active: false, percent: 0 })
  const lastImportTiming = ref<ImportTiming | null>(null)
  // ponytail: 简易 LRU，上限 10 条；超过时按插入序丢弃最早一项
  const VIDEO_INFO_CACHE_MAX = 10
  const videoInfoCache = new Map<string, VideoInfoImportResponse>()
  const setVideoInfoCache = (key: string, value: VideoInfoImportResponse) => {
    if (videoInfoCache.has(key)) videoInfoCache.delete(key)
    videoInfoCache.set(key, value)
    while (videoInfoCache.size > VIDEO_INFO_CACHE_MAX) {
      const oldest = videoInfoCache.keys().next().value
      if (!oldest) break
      videoInfoCache.delete(oldest)
    }
  }
  const getVideoInfoCache = (key: string) => {
    const v = videoInfoCache.get(key)
    if (v) {
      videoInfoCache.delete(key)
      videoInfoCache.set(key, v)
    }
    return v
  }

  const makeCacheKey = (paths: string[], isRecursive: boolean) =>
    JSON.stringify({ recursive: isRecursive, paths: [...paths].sort() })

  const importByPaths = async (paths: string[], overrideRecursive?: boolean) => {
    if (!paths || paths.length === 0) return
    const startedAt = performance.now()
    const isRecursive = overrideRecursive ?? recursive.value
    const cacheKey = makeCacheKey(paths, isRecursive)
    const cached = getVideoInfoCache(cacheKey)
    if (cached) {
      const metadataFinishedAt = performance.now()
      items.value = cached.items
      importSummary.value = `${t('成功')} ${cached.success}，${t('失败')} ${cached.failed}，${t('共')} ${cached.total} ${t('个视频')}`
      expandedIds.value.clear()
      await nextTick()
      const displayedAt = performance.now()
      lastImportTiming.value = {
        metadataMs: metadataFinishedAt - startedAt,
        displayMs: displayedAt - metadataFinishedAt,
        totalMs: displayedAt - startedAt,
        cached: true,
      }
      void recordMediaPerformance({
        mode: 'detailed',
        inputCount: paths.length,
        total: cached.total,
        success: cached.success,
        failed: cached.failed,
        ...lastImportTiming.value,
      }).catch(() => undefined)
      ElMessage.success(t('已使用本次会话缓存结果'))
      return
    }

    importing.value = true
    importSummary.value = ''
    expandedIds.value.clear()

    try {
      const resp = await importDetailedVideoInfo(paths, isRecursive)
      const metadataFinishedAt = performance.now()
      setVideoInfoCache(cacheKey, resp)
      items.value = resp.items
      importSummary.value = `${t('成功')} ${resp.success}，${t('失败')} ${resp.failed}，${t('共')} ${resp.total} ${t('个视频')}`
      await nextTick()
      const displayedAt = performance.now()
      lastImportTiming.value = {
        metadataMs: metadataFinishedAt - startedAt,
        displayMs: displayedAt - metadataFinishedAt,
        totalMs: displayedAt - startedAt,
        cached: false,
      }
      void recordMediaPerformance({
        mode: 'detailed',
        inputCount: paths.length,
        total: resp.total,
        success: resp.success,
        failed: resp.failed,
        ...lastImportTiming.value,
      }).catch(() => undefined)

      if (resp.failed > 0) {
        ElMessage.warning(importSummary.value)
      } else {
        ElMessage.success(importSummary.value)
      }
    } catch (e: any) {
      ElMessage.error(e?.toString() || t('导入失败'))
    } finally {
      importing.value = false
      importProgress.active = false
      importProgress.percent = 0
    }
  }

  const handleImport = async (kind: 'folder' | 'clipboard') => {
    const picked = await pick(kind)
    const paths = (picked || []).filter((p: string) => !/[\*\?\[\]]/.test(p))
    return importByPaths(paths)
  }

  // ==================== 清空 ====================
  const handleClear = () => {
    items.value = []
    expandedIds.value.clear()
    importSummary.value = ''
    lastImportTiming.value = null
    searchQuery.value = ''
  }

  // ==================== 加入重命名 ====================
  const handleAddToRename = () => {
    const successItems = items.value.filter((i) => i.status === 'success')
    if (!successItems.length) {
      ElMessage.info(t('没有可加入的视频'))
      return
    }
    emit('addToRename', successItems)
  }

  // ==================== 数据库入库 ====================
  const saving = ref(false)
  const saveProgress = reactive({ total: 0, saved: 0, phase: 'xml' as 'xml' | 'db' | 'done' })

  const handleSaveToDb = async () => {
    if (!items.value.length) return
    const successItems = items.value.filter(i => i.status === 'success')
    if (!successItems.length) {
      ElMessage.info(t('没有可入库的视频'))
      return
    }
    saving.value = true
    saveProgress.total = successItems.length
    saveProgress.saved = 0
    saveProgress.phase = 'xml'

    try {
      const xmlMap = new Map<string, string>()

      const CONCURRENCY = 4
      let completed = 0
      const fetchXml = async (item: VideoInfoItem) => {
        try {
          const xml = await getVideoRawXml(item.path)
          if (xml) xmlMap.set(item.path, xml)
        } catch { /* 跳过 */ }
        completed++
        saveProgress.saved = completed
      }
      const queue = [...successItems]
      const workers = Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
        while (queue.length) {
          const item = queue.shift()!
          await fetchXml(item)
        }
      })
      await Promise.allSettled(workers)

      saveProgress.phase = 'db'
      const count = await saveVideoRecords(items.value, xmlMap)

      saveProgress.phase = 'done'
      ElMessage.success(`${t('已保存')} ${count} ${t('条记录到数据库')}`)
      await refreshHistoryCount()
    } catch (e: any) {
      ElMessage.error(`${t('入库失败')}：${e?.message || e}`)
    } finally {
      saving.value = false
    }
  }

  // ==================== 历史记录 ====================
  const historyVisible = ref(false)
  const historyRecords = shallowRef<VideoRecordRow[]>([])
  const historyLoading = ref(false)
  const historySelection = ref<string[]>([])
  const historyCount = ref(0)
  const historyPageSize = 50
  const historyHasMore = ref(true)
  // 历史记录列选择
  const historyColKeys = ref<string[]>(['name', 'format', 'resolution', 'codec', 'duration', 'overall_bit_rate', 'scanned_at', 'size'])

  // 历史记录列定义
  type HistoryColDef = { key: string; label: string; width?: number; group: 'basic' | 'video' | 'audio' | 'extra' }
  const HISTORY_ALL_COLUMNS: HistoryColDef[] = [
    { key: 'name', label: '文件名', width: 200, group: 'basic' },
    { key: 'format', label: '格式', width: 80, group: 'basic' },
    { key: 'resolution', label: '分辨率', width: 120, group: 'basic' },
    { key: 'codec', label: '编码', width: 100, group: 'basic' },
    { key: 'frame_rate', label: '帧率', width: 80, group: 'basic' },
    { key: 'duration', label: '时长', width: 90, group: 'basic' },
    { key: 'overall_bit_rate', label: '码率', width: 120, group: 'basic' },
    { key: 'size', label: '文件大小', width: 100, group: 'basic' },
    { key: 'scanned_at', label: '入库时间', width: 160, group: 'basic' },
    { key: 'bit_depth', label: '位深', width: 80, group: 'video' },
    { key: 'hdr_format', label: 'HDR', width: 150, group: 'video' },
    { key: 'scan_type', label: '扫描方式', width: 90, group: 'video' },
    { key: 'color_space', label: '色彩空间', width: 90, group: 'video' },
    { key: 'color_primaries', label: '色域', width: 100, group: 'video' },
    { key: 'transfer_characteristics', label: '传输特性', width: 110, group: 'video' },
    { key: 'chroma_subsampling', label: '色度采样', width: 90, group: 'video' },
    { key: 'format_profile', label: '编码Profile', width: 120, group: 'video' },
    { key: 'video_bit_rate', label: '视频码率', width: 110, group: 'video' },
    { key: 'video_stream_size', label: '视频流大小', width: 110, group: 'video' },
    { key: 'video_language', label: '视频语言', width: 100, group: 'video' },
    { key: 'channels', label: '声道', width: 100, group: 'audio' },
    { key: 'channel_layout', label: '声道布局', width: 120, group: 'audio' },
    { key: 'audio_codec', label: '音频编码', width: 110, group: 'audio' },
    { key: 'audio_bit_rate', label: '音频码率', width: 110, group: 'audio' },
    { key: 'sample_rate', label: '采样率', width: 90, group: 'audio' },
    { key: 'audio_language', label: '音频语言', width: 100, group: 'audio' },
    { key: 'audio_stream_size', label: '音频流大小', width: 110, group: 'audio' },
    { key: 'text_count', label: '字幕数', width: 80, group: 'extra' },
    { key: 'text_languages', label: '字幕语言', width: 150, group: 'extra' },
    { key: 'writing_application', label: '封装工具', width: 150, group: 'extra' },
    { key: 'encoded_library', label: '编码库', width: 150, group: 'extra' },
    { key: 'path', label: '文件路径', width: 300, group: 'extra' },
  ]
  const HISTORY_DEFAULT_KEYS = ['name', 'format', 'resolution', 'codec', 'duration', 'overall_bit_rate', 'scanned_at', 'size']

  const historyColPopoverVisible = ref(false)
  const GROUP_LABELS: Record<string, string> = { basic: '基本信息', video: '视频流', audio: '音频流', extra: '其他' }
  const historyGroupedCols = computed(() => {
    const groups: Record<string, HistoryColDef[]> = { basic: [], video: [], audio: [], extra: [] }
    HISTORY_ALL_COLUMNS.forEach(c => { const g = groups[c.group]; if (g) g.push(c) })
    return groups
  })

  // 从 detail_json 提取额外字段（Map 缓存）
  const detailCache = new Map<string, any>()
  const DETAIL_CACHE_MAX = 200
  const getExtraField = (row: VideoRecordRow, key: string): string => {
    if (!row.detail_json) return '-'
    try {
      let d = detailCache.get(row.id)
      if (d === undefined) {
        d = JSON.parse(row.detail_json)
        if (detailCache.size >= DETAIL_CACHE_MAX) {
          const firstKey = detailCache.keys().next().value
          if (firstKey) detailCache.delete(firstKey)
        }
        detailCache.set(row.id, d)
      }
      const g = d.general ?? {}
      const v = d.videoStreams?.[0] ?? {}
      const a = d.audioStreams?.[0] ?? {}
      const texts: any[] = d.textStreams ?? []
      switch (key) {
        case 'bit_depth': return v.bitDepth ?? '-'
        case 'hdr_format': return v.hdrFormat ?? '-'
        case 'scan_type': return v.scanType ?? '-'
        case 'color_space': return v.colorSpace ?? '-'
        case 'color_primaries': return v.colorPrimaries ?? '-'
        case 'transfer_characteristics': return v.transferCharacteristics ?? '-'
        case 'chroma_subsampling': return v.chromaSubsampling ?? '-'
        case 'format_profile': return v.formatProfile ?? '-'
        case 'video_bit_rate': return v.bitRate ?? '-'
        case 'video_stream_size': return v.streamSize ?? '-'
        case 'video_language': return v.language ?? '-'
        case 'channels': return a.channels ?? '-'
        case 'channel_layout': return a.channelLayout ?? '-'
        case 'audio_codec': return a.codec ?? '-'
        case 'audio_bit_rate': return a.bitRate ?? '-'
        case 'sample_rate': return a.sampleRate ?? '-'
        case 'audio_language': return a.language ?? '-'
        case 'audio_stream_size': return a.streamSize ?? '-'
        case 'text_count': return texts.length ? String(texts.length) : '-'
        case 'text_languages': {
          const langs = [...new Set(texts.map((t: any) => t.language).filter(Boolean))]
          return langs.length ? langs.join(', ') : '-'
        }
        case 'writing_application': return g.writingApplication ?? '-'
        case 'encoded_library': return v.encodedLibrary || g.encodedLibrary || '-'
        case 'path': return row.path ?? '-'
        default: return '-'
      }
    } catch { return '-' }
  }

  // 格式化时长
  const formatDurationMs = (ms: number): string => formatDuration(ms / 1000, 'clock')

  const refreshHistoryCount = async () => {
    try {
      historyCount.value = await getRecordCount()
    } catch { historyCount.value = 0 }
  }

  const openHistory = async () => {
    historyVisible.value = true
    historyLoading.value = true
    historyHasMore.value = true
    try {
      historyRecords.value = await loadVideoRecords(historyPageSize, 0)
      historyHasMore.value = historyRecords.value.length >= historyPageSize
    } catch (e: any) {
      ElMessage.error(`${t('加载历史记录失败')}：${e?.message || e}`)
    } finally {
      historyLoading.value = false
    }
  }

  const loadMoreHistory = async () => {
    if (historyLoading.value || !historyHasMore.value) return
    historyLoading.value = true
    try {
      const more = await loadVideoRecords(historyPageSize, historyRecords.value.length)
      historyRecords.value = [...historyRecords.value, ...more]
      historyHasMore.value = more.length >= historyPageSize
    } catch (e: any) {
      ElMessage.error(`${t('加载历史记录失败')}：${e?.message || e}`)
    } finally {
      historyLoading.value = false
    }
  }

  const loadHistoryToView = () => {
    const rows = historySelection.value.length
      ? historyRecords.value.filter(r => historySelection.value.includes(r.id))
      : historyRecords.value

    items.value = rows.map(rowToVideoInfoItem)
    importSummary.value = `${t('从历史记录加载')} ${items.value.length} ${t('条')}`
    expandedIds.value.clear()
    historyVisible.value = false
    ElMessage.success(`${t('已加载')} ${items.value.length} ${t('条历史记录')}`)
  }

  const deleteSelectedHistory = async () => {
    if (!historySelection.value.length) return
    try {
      await deleteVideoRecords(historySelection.value)
      historyRecords.value = historyRecords.value.filter(r => !historySelection.value.includes(r.id))
      historySelection.value = []
      await refreshHistoryCount()
      ElMessage.success(t('删除成功'))
    } catch (e: any) {
      ElMessage.error(`${t('删除失败')}：${e?.message || e}`)
    }
  }

  // ==================== 导出 ====================
  type OutputMode = 'same' | 'folder'
  const outputModeDialogVisible = ref(false)
  const pendingExportFormat = ref<ExportFormat | null>(null)
  const outputMode = ref<OutputMode>('same')
  const customOutputDir = ref('')

  const exportRows = computed(() => preFilterTableData.value)

  const exportHeaders = computed(() => [t('序号'), t('文件名'), ...visibleColumns.value.map(c => t(c.label)), t('文件大小'), t('流')])

  const exportDataRows = computed(() =>
    exportRows.value.map((row: any, idx: number) => {
      const cells: (string | number)[] = [
        idx + 1,
        row.name,
        ...visibleColumns.value.map(c => {
          const v = (row as any)[c.prop]
          if (c.prop === 'healthIssueCount') return row.healthLabel
          return v ?? '-'
        }),
        formatBytes(row.size),
        [
          row.videoCount ? `V${row.videoCount}` : '',
          row.audioCount ? `A${row.audioCount}` : '',
          row.textCount ? `S${row.textCount}` : '',
        ].filter(Boolean).join(' '),
      ]
      return cells
    })
  )

  const formatIssueText = (issues: HealthIssue[]) => issues.map((issue) => `${issue.label}: ${issue.detail}`).join('; ') || '正常'

  const buildMarkdownReport = () => {
    const lines = [
      '# 视频元数据报告',
      '',
      `生成时间：${new Date().toLocaleString()}`,
      '',
      `- 文件数：${items.value.length}`,
      `- 正常文件：${healthSummary.value.cleanFiles}`,
      `- 严重问题：${healthSummary.value.danger}`,
      `- 警告：${healthSummary.value.warning}`,
      `- 提示：${healthSummary.value.info}`,
      '',
      '| 文件 | 分辨率 | 编码 | 码率 | 帧率 | 体检结果 |',
      '|---|---:|---|---:|---:|---|'
    ]

    exportRows.value.forEach((row: any) => {
      lines.push(`| ${row.name} | ${row.resolution} | ${row.codec} | ${row.bitrate} | ${row.frameRate} | ${formatIssueText(row.healthIssues)} |`)
    })
    return lines.join('\n')
  }

  const buildHtmlReport = () => {
    const rows = exportRows.value.map((row: any) => `
      <tr>
        <td>${escapeHtml(row.name)}</td>
        <td>${escapeHtml(row.resolution)}</td>
        <td>${escapeHtml(row.codec)}</td>
        <td>${escapeHtml(row.bitrate)}</td>
        <td>${escapeHtml(row.frameRate)}</td>
        <td>${escapeHtml(formatIssueText(row.healthIssues))}</td>
      </tr>
    `).join('')
    return `<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <title>视频元数据报告</title>
  <style>
    body{font-family:Arial,"Microsoft YaHei",sans-serif;margin:28px;color:#1f2430}
    h1{margin-bottom:8px}
    .meta{color:#667085;margin-bottom:18px}
    .summary{display:flex;gap:10px;flex-wrap:wrap;margin:16px 0}
    .pill{padding:6px 10px;border-radius:8px;background:#f2f4f7}
    table{width:100%;border-collapse:collapse;font-size:13px}
    th,td{border:1px solid #d0d5dd;padding:8px;text-align:left;vertical-align:top}
    th{background:#f8fafc}
  </style>
</head>
<body>
  <h1>视频元数据报告</h1>
  <div class="meta">生成时间：${escapeHtml(new Date().toLocaleString())}</div>
  <div class="summary">
    <span class="pill">文件数：${items.value.length}</span>
    <span class="pill">正常：${healthSummary.value.cleanFiles}</span>
    <span class="pill">严重：${healthSummary.value.danger}</span>
    <span class="pill">警告：${healthSummary.value.warning}</span>
    <span class="pill">提示：${healthSummary.value.info}</span>
  </div>
  <table>
    <thead><tr><th>文件</th><th>分辨率</th><th>编码</th><th>码率</th><th>帧率</th><th>体检结果</th></tr></thead>
    <tbody>${rows}</tbody>
  </table>
</body>
</html>`
  }

  const generateOutputDirName = (format: ExportFormat): string => {
    const extMap: Record<string, string> = { txt: 'txt', xml2json: 'json', xml2md: 'md' }
    const ext = extMap[format] || format
    const now = new Date()
    const pad = (n: number) => String(n).padStart(2, '0')
    const timestamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`
    return `mediainfo_${ext}_${timestamp}`
  }

  const confirmOutputMode = async () => {
    outputModeDialogVisible.value = false
    const format = pendingExportFormat.value
    if (!format) return
    if (outputMode.value === 'folder' && !customOutputDir.value) {
      const dirName = generateOutputDirName(format)
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: '选择输出目录',
        defaultPath: dirName
      })
      if (!selected) return
      customOutputDir.value = selected as string
    }
    await doBatchExport(format)
  }

  const doBatchExport = async (format: ExportFormat) => {
    const extMap: Record<string, string> = { txt: 'txt', xml2json: 'json', xml2md: 'md' }
    const ext = extMap[format]

    const validItems = items.value.filter(i => i.status === 'success' && i.detail)
    if (!validItems.length) {
      ElMessage.warning(t('没有可导出的 MediaInfo 数据'))
      return
    }

    const getExportFn = (path: string) => {
      if (format === 'txt') return getVideoCompleteInfo(path)
      if (format === 'xml2json') return getVideoXmlJson(path)
      return getVideoXmlMarkdown(path)
    }

    try {
      if (outputMode.value === 'same') {
        let count = 0
        for (const item of validItems) {
          const result = await getExportFn(item.path)
          if (!result) continue
          const baseName = item.name.replace(/\.[^.]+$/, '')
          const dir = item.path.replace(/[/\\][^/\\]+$/, '')
          const outPath = `${dir}/${baseName}_MediaInfo.${ext}`
          await writeTextExportFile(outPath, result)
          count++
        }
        ElMessage.success(`导出完成（${count} 个文件，已保存到源文件所在目录）`)
      } else {
        const outDir = customOutputDir.value
        if (!outDir) return

        if (format === 'xml2json') {
          const results: any[] = []
          for (const item of validItems) {
            const result = await getVideoXmlJson(item.path)
            if (result) {
              try { results.push(JSON.parse(result)) } catch { /* skip */ }
            }
          }
          const outPath = `${outDir}/MediaInfo_ALL.json`
          await writeTextExportFile(outPath, JSON.stringify(results, null, 2))
          ElMessage.success(`导出完成（${results.length} 个文件 → ${outPath}）`)
        } else if (format === 'xml2md') {
          const header = `# MediaInfo 批量导出报告\n\n**导出时间:** ${new Date().toLocaleString()}\n**文件数量:** ${validItems.length}\n\n---\n\n`
          const parts: string[] = []
          for (const item of validItems) {
            const result = await getVideoXmlMarkdown(item.path)
            if (result) parts.push(result)
          }
          const outPath = `${outDir}/MediaInfo_ALL.md`
          await writeTextExportFile(outPath, header + parts.join('\n\n---\n\n'))
          ElMessage.success(`导出完成（${parts.length} 个文件 → ${outPath}）`)
        } else {
          let count = 0
          for (const item of validItems) {
            const result = await getVideoCompleteInfo(item.path)
            if (!result) continue
            const baseName = item.name.replace(/\.[^.]+$/, '')
            const outPath = `${outDir}/${baseName}_MediaInfo.txt`
            await writeTextExportFile(outPath, result)
            count++
          }
          ElMessage.success(`导出完成（${count} 个文件 → ${outDir}）`)
        }
      }
    } catch (e: any) {
      ElMessage.error(`导出失败：${e?.message || e}`)
    }
  }

  const exportData = async (format: ExportFormat) => {
    const rows = flatTableData.value
    if (!rows.length) {
      ElMessage.warning(t('没有数据可导出'))
      return
    }

    if (format === 'txt' || format === 'xml2json' || format === 'xml2md') {
      pendingExportFormat.value = format
      customOutputDir.value = ''
      outputModeDialogVisible.value = true
      return
    }

    const extensionMap: Record<string, string> = {
      xlsx: 'xlsx', csv: 'csv', markdown: 'md', html: 'html', json: 'json'
    }
    const defaultName = `video-health-${Date.now()}.${extensionMap[format]}`
    const filters: Record<string, { name: string; extensions: string[] }[]> = {
      xlsx: [{ name: 'Excel', extensions: ['xlsx'] }],
      csv: [{ name: 'CSV', extensions: ['csv'] }],
      markdown: [{ name: 'Markdown', extensions: ['md'] }],
      html: [{ name: 'HTML', extensions: ['html'] }],
      json: [{ name: 'JSON', extensions: ['json'] }]
    }
    const filePath = await save({
      defaultPath: defaultName,
      filters: filters[format],
    })
    if (!filePath) return

    const headers = exportHeaders.value
    const dataRows = exportDataRows.value

    try {
      if (format === 'csv') {
        const bom = '﻿'
        const csv = bom + [headers.map(csvEscape).join(','), ...dataRows.map(r => r.map(csvEscape).join(','))].join('\n')
        await writeTextExportFile(filePath, csv)
      } else if (format === 'markdown') {
        await writeTextExportFile(filePath, buildMarkdownReport())
      } else if (format === 'html') {
        await writeTextExportFile(filePath, buildHtmlReport())
      } else if (format === 'json') {
        await writeTextExportFile(filePath, JSON.stringify({
          generatedAt: new Date().toISOString(),
          summary: healthSummary.value,
          rows: exportRows.value.map((row: any) => ({
            ...row,
            healthIssues: row.healthIssues
          }))
        }, null, 2))
      } else {
        const ExcelJS = await import('exceljs')
        const wb = new ExcelJS.Workbook()
        const ws = wb.addWorksheet('Video Info')
        ws.addRow(headers)
        dataRows.forEach((r: any) => ws.addRow(r))

        ws.getRow(1).eachCell(cell => {
          cell.font = { bold: true }
          cell.fill = { type: 'pattern', pattern: 'solid', fgColor: { argb: 'FFE8EEFF' } }
          cell.border = {
            bottom: { style: 'thin', color: { argb: 'FFD0D5DD' } },
          }
        })

        ws.columns.forEach((col, i) => {
          const maxLen = Math.max(
            headers[i]?.length || 8,
            ...dataRows.map((r: any) => String(r[i] || '').length)
          )
          col.width = Math.min(Math.max(maxLen + 2, 8), 40)
        })

        const buf = await wb.xlsx.writeBuffer()
        await writeBinaryExportFile(filePath, Array.from(new Uint8Array(buf)))
      }

      ElMessage.success(`${t('导出成功')}（${rows.length} ${t('行')}）\n${filePath}`)
    } catch (e: any) {
      ElMessage.error(`${t('导出失败')}：${e?.message || e}`)
    }
  }

  // ==================== 统计 ====================
  const successCount = computed(() => items.value.filter((i) => i.status === 'success').length)
  const failedCount = computed(() => items.value.filter((i) => i.status !== 'success').length)

  return {
    // 导入
    importing,
    importProgress,
    lastImportTiming,
    importByPaths,
    handleImport,

    // 清空/操作
    handleClear,
    handleAddToRename: handleAddToRename,

    // 入库
    saving,
    saveProgress,
    handleSaveToDb,

    // 历史
    historyVisible,
    historyRecords,
    historyLoading,
    historySelection,
    historyCount,
    historyPageSize,
    historyHasMore,
    historyColKeys,
    historyColPopoverVisible,
    HISTORY_ALL_COLUMNS,
    HISTORY_DEFAULT_KEYS,
    GROUP_LABELS,
    historyGroupedCols,
    getExtraField,
    formatDurationMs,
    refreshHistoryCount,
    openHistory,
    loadMoreHistory,
    loadHistoryToView,
    deleteSelectedHistory,

    // 导出
    outputModeDialogVisible,
    pendingExportFormat,
    outputMode,
    customOutputDir,
    confirmOutputMode,
    exportData,
    exportRows,
    exportHeaders,
    exportDataRows,

    // 统计
    successCount,
    failedCount,
  }
}
