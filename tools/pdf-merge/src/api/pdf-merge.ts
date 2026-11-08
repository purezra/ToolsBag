import { invoke } from '@tauri-apps/api/core'

/** 合并输入类型 */
export type MergeInputKind = 'pdf' | 'image' | 'doc'

/** 单个输入文件的分析结果 */
export interface MergeInputInfo {
  path: string
  name: string
  kind: MergeInputKind | 'unknown'
  extension: string
  page_count: number
  file_size: number
  /** 修改时间（Unix 毫秒） */
  modified_ms: number
  width?: number
  height?: number
  /** 分析失败原因（前端提示并跳过该文件） */
  error?: string
}

/** 合并请求单项：数组顺序即最终页面顺序 */
export interface MergeItemRequest {
  path: string
  /** 仅 Word 文档使用；留空表示全部页面，如 "1,5,7" / "1-3,5" */
  page_selection?: string | null
}

export interface MergeProgress {
  current: number
  total: number
  current_file: string
  /** pdf / image / convert / done */
  phase: string
}

export interface MergeResult {
  success: boolean
  output_path: string
  file_size: number
  page_count: number
  source_count: number
  elapsed_ms: number
}

export interface OfficeConverterStatus {
  libreoffice: string | null
  word: boolean
  available: boolean
}

/** 支持导入的扩展名（与后端检测保持一致） */
export const MERGE_EXTENSIONS = [
  'pdf',
  'jpg',
  'jpeg',
  'png',
  'webp',
  'tif',
  'tiff',
  'bmp',
  'gif',
  'doc',
  'docx',
] as const

/** 分析一批文件：PDF 页数、图片尺寸、Word 转换后的页数 */
export async function analyzeMergeInputs(paths: string[]): Promise<MergeInputInfo[]> {
  return invoke<MergeInputInfo[]>('analyze_merge_inputs', { paths })
}

/** 按给定顺序合并并写入 outputPath */
export async function mergePdfDocuments(
  items: MergeItemRequest[],
  outputPath: string
): Promise<MergeResult> {
  return invoke<MergeResult>('merge_pdf_documents', { items, outputPath })
}

/** 检测 LibreOffice / Microsoft Word 是否可用于 DOC/DOCX 转换 */
export async function getOfficeConverterStatus(): Promise<OfficeConverterStatus> {
  return invoke<OfficeConverterStatus>('get_office_converter_status')
}

/** 把生成的 PDF 以文件形式复制到剪贴板（Windows） */
export async function copyPdfFile(path: string): Promise<void> {
  return invoke<void>('copy_pdf_file', { path })
}
