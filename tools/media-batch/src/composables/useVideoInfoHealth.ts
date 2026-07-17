import { computed, type Ref } from 'vue'
import type { HealthIssue, HealthSeverity, VideoInfoItem } from '../types/media'
import { parseBitRateMbps } from '../utils/media-utils'

// ==================== 语言映射 ====================

/** MediaInfo 语言名称 -> 中文映射 */
const langMap: Record<string, string> = {
  'chinese': '中文',
  'chinese (simplified)': '简体中文',
  'chinese (traditional)': '繁体中文',
  'english': '英文',
  'japanese': '日语',
  'korean': '韩语',
  'french': '法语',
  'german': '德语',
  'spanish': '西班牙语',
  'russian': '俄语',
  'arabic': '阿拉伯语',
  'portuguese': '葡萄牙语',
  'italian': '意大利语',
  'thai': '泰语',
  'vietnamese': '越南语',
  'indonesian': '印尼语',
  'malay': '马来语',
  'hindi': '印地语',
  'dutch': '荷兰语',
  'polish': '波兰语',
  'turkish': '土耳其语',
  'swedish': '瑞典语',
  'norwegian': '挪威语',
  'danish': '丹麦语',
  'finnish': '芬兰语',
  'greek': '希腊语',
  'hebrew': '希伯来语',
  'czech': '捷克语',
  'romanian': '罗马尼亚语',
  'hungarian': '匈牙利语',
  'ukrainian': '乌克兰语',
}

const translateLang = (lang: string): string => {
  if (!lang) return lang
  return langMap[lang.toLowerCase()] || lang
}

// ==================== 格式解析工具 ====================

/** 从 MediaInfo 原始字符串中提取第一个数字（支持整数/小数） */
const parseFirstNum = (s: string): number => {
  if (!s) return 0
  const m = s.match(/[\d]+(?:\.[\d]+)?/)
  return m ? parseFloat(m[0]) : 0
}

/** 解析声道数："6 channels" → "6ch", "2 / 6 channels" → "2ch", "6" → "6ch" */
const parseChannels = (s: string): string => {
  if (!s) return ''
  const n = parseFirstNum(s)
  return n > 0 ? `${Math.round(n)}ch` : ''
}

/** 解析采样率："48000" → "48kHz", "48000 Hz" → "48kHz", "48.0 kHz" → "48kHz" */
const parseSampleRate = (s: string): string => {
  if (!s) return ''
  const lower = s.toLowerCase()
  if (lower.includes('khz')) {
    const n = parseFirstNum(s)
    return n > 0 ? `${Math.round(n)}kHz` : ''
  }
  const n = parseFirstNum(s)
  if (n <= 0) return ''
  if (n >= 1000) return `${Math.round(n / 1000)}kHz`
  return `${Math.round(n)}kHz`
}

/** 解析码率："562000" → "562kbps" */
const parseBitRate = (s: string): string => {
  if (!s) return ''
  const lower = s.toLowerCase()
  if (lower.includes('kb') || lower.includes('kbit')) {
    const n = parseFirstNum(s)
    return n > 0 ? `${Math.round(n)}kbps` : ''
  }
  const n = parseFirstNum(s)
  if (n <= 0) return ''
  if (n >= 10000) return `${Math.round(n / 1000)}kbps`
  return `${Math.round(n)}kbps`
}

const isCommonFrameRate = (value: number): boolean => {
  if (value <= 0) return true
  return [23.976, 24, 25, 29.97, 30, 50, 59.94, 60, 120].some((common) => Math.abs(value - common) < 0.12)
}

// ==================== 音轨/字幕摘要构建 ====================

/** 音轨摘要（入门级）：编码 + 声道 + 采样率 */
const buildAudioSummary = (item: VideoInfoItem): string => {
  const streams = item.detail?.audioStreams
  if (!streams || streams.length === 0) return '-'
  const first = streams[0]!
  const codec = first.codec || '?'
  const ch = parseChannels(first.channels)
  const sr = parseSampleRate(first.sampleRate)
  return [codec, ch, sr].filter(Boolean).join(' ')
}

/** 字幕摘要（入门级）：优先显示语言，无语言时显示格式 */
const buildTextSummary = (item: VideoInfoItem): string => {
  const streams = item.detail?.textStreams
  if (!streams || streams.length === 0) return '-'
  return streams.map(s => {
    if (s.language) return translateLang(s.language)
    if (s.title) return s.title
    return s.format || '?'
  }).join(' / ')
}

/** 音轨详情（专业级）：编码 + 声道 + 采样率 + 码率 + 语言 */
const buildAudioDetail = (item: VideoInfoItem): string => {
  const streams = item.detail?.audioStreams
  if (!streams || streams.length === 0) return '-'
  return streams.map((s, i) => {
    const codec = s.codec || '?'
    const ch = parseChannels(s.channels)
    const sr = parseSampleRate(s.sampleRate)
    const br = parseBitRate(s.bitRate)
    const lang = s.language ? `[${translateLang(s.language)}]` : ''
    return `#${i + 1} ${[codec, ch, sr, br, lang].filter(Boolean).join(' ')}`
  }).join('  ')
}

/** 字幕详情（专业级） */
const buildTextDetail = (item: VideoInfoItem): string => {
  const streams = item.detail?.textStreams
  if (!streams || streams.length === 0) return '-'
  return streams.map((s, i) => {
    const fmt = s.format || '?'
    const lang = s.language ? `[${translateLang(s.language)}]` : ''
    return `#${i + 1} ${fmt} ${lang}`.trim()
  }).join('  ')
}

// ==================== 体检分析 ====================

const getWorstSeverity = (issues: HealthIssue[]): HealthSeverity | 'success' => {
  if (issues.some((issue) => issue.severity === 'danger')) return 'danger'
  if (issues.some((issue) => issue.severity === 'warning')) return 'warning'
  if (issues.some((issue) => issue.severity === 'info')) return 'info'
  return 'success'
}

const getSeverityTagType = (severity: HealthSeverity | 'success') => {
  if (severity === 'danger') return 'danger'
  if (severity === 'warning') return 'warning'
  if (severity === 'info') return 'info'
  return 'success'
}

const analyzeHealth = (item: VideoInfoItem): HealthIssue[] => {
  const issues: HealthIssue[] = []
  if (item.status !== 'success') {
    issues.push({ severity: 'danger', label: '读取失败', detail: item.reason || '元数据读取失败' })
    return issues
  }

  const detail = item.detail
  if (!detail) {
    issues.push({ severity: 'danger', label: '缺少详情', detail: '没有拿到 MediaInfo 详情数据' })
    return issues
  }

  const video = detail.videoStreams?.[0]
  const audioStreams = detail.audioStreams || []
  const textStreams = detail.textStreams || []
  if (!video) {
    issues.push({ severity: 'danger', label: '无视频流', detail: '文件没有可识别的视频流' })
    return issues
  }

  const bitrateMbps = parseBitRateMbps(video.bitRate || detail.general?.overallBitRate || '')
  const width = video.width || 0
  const frameRate = parseFirstNum(video.frameRate || '')
  const bitDepth = parseFirstNum(video.bitDepth || '')
  const durationMs = video.durationMs || detail.general?.durationMs || 0

  if (!durationMs) issues.push({ severity: 'warning', label: '时长缺失', detail: '无法识别有效时长' })
  if (!audioStreams.length) issues.push({ severity: 'warning', label: '无音轨', detail: '未检测到音频流' })
  if (!textStreams.length) issues.push({ severity: 'info', label: '无字幕', detail: '未检测到字幕流' })
  if (width >= 3840 && bitrateMbps > 0 && bitrateMbps < 12) {
    issues.push({ severity: 'warning', label: '4K低码率', detail: `4K 视频码率约 ${bitrateMbps.toFixed(2)} Mbps` })
  } else if (width >= 1920 && bitrateMbps > 0 && bitrateMbps < 3) {
    issues.push({ severity: 'warning', label: '1080p低码率', detail: `1080p 视频码率约 ${bitrateMbps.toFixed(2)} Mbps` })
  }
  if (frameRate > 0 && !isCommonFrameRate(frameRate)) {
    issues.push({ severity: 'info', label: '非常规帧率', detail: `帧率为 ${frameRate}` })
  }
  if (video.hdrFormat && video.hdrFormat !== '-' && bitDepth > 0 && bitDepth < 10) {
    issues.push({ severity: 'warning', label: 'HDR位深偏低', detail: `HDR 视频位深为 ${video.bitDepth}` })
  }
  if ((detail.videoStreams?.length || 0) > 1) {
    issues.push({ severity: 'info', label: '多视频流', detail: `检测到 ${detail.videoStreams.length} 条视频流` })
  }

  return issues
}

// ==================== Composable ====================

export function useVideoInfoHealth(items: Ref<VideoInfoItem[]>) {
  const healthIssueMap = computed(() => new Map(items.value.map((item) => [item.id, analyzeHealth(item)])))

  const healthSummary = computed(() => {
    let danger = 0
    let warning = 0
    let info = 0
    for (const issues of healthIssueMap.value.values()) {
      issues.forEach((issue) => {
        if (issue.severity === 'danger') danger++
        else if (issue.severity === 'warning') warning++
        else info++
      })
    }
    return {
      danger,
      warning,
      info,
      totalIssues: danger + warning + info,
      cleanFiles: items.value.filter((item) => (healthIssueMap.value.get(item.id) || []).length === 0).length
    }
  })

  const getHealthIssues = (id: number) => healthIssueMap.value.get(id) || []

  return {
    // computed
    healthIssueMap,
    healthSummary,

    // 方法
    getHealthIssues,
    translateLang,
    analyzeHealth,
    getWorstSeverity,
    getSeverityTagType,
    buildAudioSummary,
    buildAudioDetail,
    buildTextSummary,
    buildTextDetail,
    parseFirstNum,
    parseBitRateMbps,
    isCommonFrameRate,
    parseChannels,
    parseSampleRate,
    parseBitRate,
  }
}