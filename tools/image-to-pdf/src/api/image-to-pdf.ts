import { invoke } from '@tauri-apps/api/core'

// 页面尺寸
export type PageSize = 'A4' | 'A3' | 'B5' | 'ipad_pro'

// 固定幅面方向
export type FixedOrientation = 'portrait' | 'landscape' | 'auto'

// 页面模式（externally tagged — matches Rust serde rename_all=snake_case）
export type PageMode =
  | 'original'
  | { fixed: { orientation: FixedOrientation } }

// 合并模式（externally tagged）
export type MergeMode =
  | { lossless: { max_size_ratio: number } }
  | { portable: { target_ratio: number } }

// 图片分析结果
export interface ImageAnalysis {
  path: string
  width: number
  height: number
  is_portrait: boolean
  format: string
  has_alpha: boolean
  /** 是否为 APNG（含动画帧，PDF 只取首帧） */
  is_apng: boolean
  /** 是否为灰度图（可优化为 DeviceGray 色彩空间） */
  is_grayscale: boolean
  /** 原始位深（8 或 16），16-bit PNG 会被降采样到 8-bit */
  bit_depth: number
  /** 是否有 gAMA / sRGB chunk（gamma != 1.0 / sRGB 标记） */
  has_gamma: boolean
  /** gamma 值（如有） */
  gamma?: number
  /** 文件字节大小 */
  file_size: number
}

// 分辨率档位
export interface ResolutionBucket {
  label: string
  count: number
  min_long_edge: number
}

// 跳过的文件（识别到但暂不支持的格式）
export interface SkippedFile {
  path: string
  ext: string
  reason: string
}

// 文件夹分析结果
export interface FolderAnalysis {
  images: ImageAnalysis[]
  effective_path: string
  recursive: boolean
  auto_switched_from_file: boolean
  portrait_count: number
  landscape_count: number
  square_count: number
  suggested_orientation: string
  total_size: number
  resolution_buckets: ResolutionBucket[]
  skipped: SkippedFile[]
  /** APNG 文件数（PDF 只取首帧，动画丢失） */
  apng_count: number
  /** 16-bit PNG 文件数（降采样到 8-bit，精度有损） */
  bit16_count: number
  /** 含 gamma 信息的文件数（PDF 不内嵌 ICC/gamma，可能有轻微色偏） */
  gamma_count: number
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

// 单页布局
export interface LayoutResult {
  image_path: string
  page_width: number
  page_height: number
  margin: number
  image: ImageLayout
}

// 预览数据
export interface PreviewData {
  pages: LayoutResult[]
  page_size: PageSize
  page_mode: PageMode
  margin: number
}

// PDF 生成配置
export interface PdfConfig {
  page_size: PageSize
  page_mode: PageMode
  margin: number
  output_path: string
  merge_mode: MergeMode
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
  mode_used: string
  size_ratio: number
  exceeded_target: boolean
  error?: string
}

/** 分析文件夹中的图片 */
export async function analyzeFolderForPdf(
  folderPath: string,
  recursive: boolean = false
): Promise<FolderAnalysis> {
  return await invoke('analyze_folder_for_pdf', { folderPath, recursive })
}

/** 计算预览布局 */
export async function calculatePreviewLayout(
  images: ImageAnalysis[],
  pageSize: PageSize,
  pageMode: PageMode,
  margin: number
): Promise<PreviewData> {
  return await invoke('calculate_preview_layout', {
    images,
    pageSize,
    pageMode,
    margin
  })
}

/** 生成 PDF */
export async function generatePdf(
  images: ImageAnalysis[],
  config: PdfConfig
): Promise<GenerationResult> {
  return await invoke('generate_pdf', { images, config })
}

/** 获取图片缩略图（base64 JPEG） */
export async function getImageThumbnail(
  imagePath: string,
  maxSize: number = 512
): Promise<string> {
  return await invoke('get_image_thumbnail', { imagePath, maxSize })
}

/** 格式化文件大小 */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}

/** 页面尺寸中文名称 */
export const PAGE_SIZE_NAMES: Record<PageSize, string> = {
  A4: 'A4 (210×297mm)',
  A3: 'A3 (297×420mm)',
  B5: 'B5 (176×250mm)',
  ipad_pro: 'iPad Pro (160×233mm)'
}

/** 幅面方向中文名称 */
export const FIXED_ORIENTATION_NAMES: Record<FixedOrientation, string> = {
  portrait: '竖向',
  landscape: '横向',
  auto: '自动（按图片方向）'
}

/** 边距预设 */
export const MARGIN_PRESETS = [0, 5, 10, 20]

/** 合并模式默认值 */
export const DEFAULT_LOSSLESS_MAX_RATIO = 1.10
export const DEFAULT_PORTABLE_TARGET_RATIO = 0.50
