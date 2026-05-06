import { invoke } from '@tauri-apps/api/core'

// 页面尺寸
export type PageSize = 'A4' | 'A3' | 'B5' | 'ipad_pro'

// 页面方向
export type PageOrientation = 'portrait' | 'landscape' | 'auto'

// 图片分析结果
export interface ImageAnalysis {
  path: string
  width: number
  height: number
  is_portrait: boolean
  format: string
  has_alpha: boolean
}

// 文件夹分析结果
export interface FolderAnalysis {
  images: ImageAnalysis[]
  portrait_count: number
  landscape_count: number
  suggested_orientation: string
  total_size: number
}

// 图片布局信息
export interface ImageLayout {
  original_width: number
  original_height: number
  scaled_width: number
  scaled_height: number
  x: number
  y: number
}

// 布局计算结果
export interface LayoutResult {
  page_width: number
  page_height: number
  margin: number
  image: ImageLayout
}

// 预览数据
export interface PreviewData {
  pages: LayoutResult[]
  page_size: PageSize
  orientation: PageOrientation
  margin: number
}

// 透明通道处理模式
export type TransparentMode = 'flatten_to_white' | 'preserve'

// PDF生成配置
export interface PdfConfig {
  page_size: PageSize
  orientation: PageOrientation
  margin: number
  compression: number
  output_path: string
  transparent_mode: TransparentMode
  jpeg_quality: number
}

// 生成进度
export interface GenerationProgress {
  current: number
  total: number
  current_file: string
  phase: string
}

// 生成结果
export interface GenerationResult {
  success: boolean
  output_path: string
  file_size: number
  original_total_size: number
  page_count: number
  elapsed_ms: number
  error?: string
}

/**
 * 分析文件夹中的图片
 */
export async function analyzeFolderForPdf(folderPath: string): Promise<FolderAnalysis> {
  return await invoke('analyze_folder_for_pdf', { folderPath })
}

/**
 * 计算预览布局
 */
export async function calculatePreviewLayout(
  images: ImageAnalysis[],
  pageSize: PageSize,
  orientation: PageOrientation,
  margin: number
): Promise<PreviewData> {
  return await invoke('calculate_preview_layout', {
    images,
    pageSize,
    orientation,
    margin
  })
}

/**
 * 生成PDF
 */
export async function generatePdf(
  images: ImageAnalysis[],
  config: PdfConfig
): Promise<GenerationResult> {
  return await invoke('generate_pdf', { images, config })
}

/**
 * 获取图片缩略图（base64）
 */
export async function getImageThumbnail(
  imagePath: string,
  maxSize: number = 512
): Promise<string> {
  return await invoke('get_image_thumbnail', { imagePath, maxSize })
}

/**
 * 格式化文件大小
 */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}

/**
 * 页面尺寸中文名称
 */
export const PAGE_SIZE_NAMES: Record<PageSize, string> = {
  A4: 'A4 (210×297mm)',
  A3: 'A3 (297×420mm)',
  B5: 'B5 (176×250mm)',
  ipad_pro: 'iPad Pro (160×233mm)'
}

/**
 * 页面方向中文名称
 */
export const ORIENTATION_NAMES: Record<PageOrientation, string> = {
  portrait: '竖版',
  landscape: '横版',
  auto: '自动'
}

/**
 * 边距预设
 */
export const MARGIN_PRESETS = [0, 5, 10, 20]
