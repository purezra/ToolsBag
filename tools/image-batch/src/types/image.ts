export type ConvertReq = {
  inputDir: string
  outputDir?: string | null
  batchSize?: number | null
  losslessMerge?: boolean | null
  recursive?: boolean
}

export type ProblemItem = { path: string; reasons: string[] }

export type ConvertResp = {
  outputPath: string
  problems: ProblemItem[]
  originalBytes: number
  outputBytes: number
}

// EPUB 转换相关类型
export type EpubPageSize = 'ipadpro' | 'ipadmini' | 'a4' | 'b5'

export type EpubConvertReq = {
  inputDir: string
  outputDir?: string | null
  pageSize?: EpubPageSize | null
  recursive?: boolean
}

export type EpubConvertResp = {
  outputPath: string
  originalBytes: number
  outputBytes: number
  pageCount: number
  pageSize: string
  problems: ProblemItem[]
}

// 页面尺寸预设信息
export const EPUB_PAGE_SIZES: { value: EpubPageSize; label: string; desc: string }[] = [
  { value: 'ipadpro', label: 'iPad Pro', desc: '2452×1668 (12.9")' },
  { value: 'ipadmini', label: 'iPad Mini', desc: '2266×1488 (8.3")' },
  { value: 'a4', label: 'A4', desc: '1240×1754 (210×297mm)' },
  { value: 'b5', label: 'B5', desc: '1039×1476 (176×250mm)' },
]
