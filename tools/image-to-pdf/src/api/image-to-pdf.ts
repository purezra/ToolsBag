import { invoke } from '@tauri-apps/api/core'
export { formatBytes as formatFileSize } from '@core/utils/format'

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
  /** 是否为动画 WebP（含动画帧，PDF 只取首帧） */
  is_animated_webp?: boolean
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
  /** TIFF 帧数（多页 TIFF 的页数，非 TIFF 为 1） */
  frame_count?: number
  /** 是否含 ICC 色彩配置（iCCP 或 JPEG APP2 ICC） */
  has_icc?: boolean
  /** ICC 配置描述（如 "sRGB", "Adobe RGB (1998)" 等） */
  icc_profile?: string
  /** ICC 主版本号（4 = ICC v4，部分老旧 PDF 阅读器支持较差） */
  icc_version?: number
  /** EXIF ColorSpace 标记："srgb" / "adobe_rgb" / "uncalibrated"，无标记为 undefined */
  exif_color_space?: string
  /** 是否为长图（长宽比 > 5:1，可能生成超长页面） */
  is_long_image?: boolean
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
  /** 动画 WebP 文件数（PDF 只取首帧，动画丢失） */
  webp_animated_count?: number
  /** 16-bit PNG 文件数（降采样到 8-bit，精度有损） */
  bit16_count: number
  /** 含 gamma 信息的文件数（PDF 不内嵌 ICC/gamma，可能有轻微色偏） */
  gamma_count: number
  /** 含 ICC 色彩配置的文件数（广色域图片可能有偏色） */
  icc_count?: number
  /** ICC v4 profile 文件数（老旧阅读器支持较差，建议用兼容模式） */
  icc_v4_count?: number
  /** 长图文件数（长宽比 > 5:1，可能生成超长页面） */
  long_image_count?: number
}

// 批量模式（按子文件夹合并）的单项
export interface BatchItemAnalysis {
  /** 文件夹名（PDF 文件名的基础） */
  name: string
  /** 文件夹绝对路径 */
  path: string
  analysis: FolderAnalysis
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
  /** 是否内嵌 ICC 色彩配置（默认开启） */
  embed_icc?: boolean
  /** 色彩处理模式：preserve = 保真（内嵌 ICC）；to_srgb = 兼容（统一转 sRGB） */
  color_mode?: ColorMode
}

/** 色彩处理模式 */
export type ColorMode = 'preserve' | 'to_srgb'

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
  /** 跳过的文件及原因（超大图、损坏等） */
  warnings?: string[]
  error?: string
}

// 批量生成结果（每个子文件夹一项）
export interface BatchGenerationResult {
  /** 实际使用的 PDF 文件名（不含 .pdf，可能已被净化/去重） */
  name: string
  /** 输出文件绝对路径 */
  output_path: string
  success: boolean
  result?: GenerationResult
  error?: string
}

/** 分析文件夹中的图片 */
export async function analyzeFolderForPdf(
  folderPath: string,
  recursive: boolean = false
): Promise<FolderAnalysis> {
  return await invoke('analyze_folder_for_pdf', { folderPath, recursive })
}

/** 批量模式：扫描一级子文件夹，每个含图片的子文件夹一个分析项（内部递归） */
export async function analyzeSubfoldersForPdf(folderPath: string): Promise<BatchItemAnalysis[]> {
  return await invoke('analyze_subfolders_for_pdf', { folderPath })
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

/** 批量生成过程中逐条推送的事件负载（事件名：pdf_batch_item） */
export interface BatchItemEvent {
  /** 对应 analyzeSubfoldersForPdf 返回数组中的下标 */
  index: number
  item: BatchGenerationResult
}

/**
 * 批量生成：每个子文件夹一份 PDF，输出到指定目录，以子文件夹名命名。
 * `overwriteExisting` 为 false（默认）时，输出目录里已有同名 PDF 会自动加序号，不会覆盖。
 */
export async function generatePdfBatch(
  items: BatchItemAnalysis[],
  config: PdfConfig,
  outputDir: string,
  overwriteExisting = false
): Promise<BatchGenerationResult[]> {
  return await invoke('generate_pdf_batch', {
    items,
    config,
    outputDir,
    overwriteExisting,
  })
}

/** 获取图片缩略图（base64 JPEG） */
export async function getImageThumbnail(
  imagePath: string,
  maxSize: number = 512
): Promise<string> {
  return await invoke('get_image_thumbnail', { imagePath, maxSize })
}

/** 页面尺寸中文名称 */
export const PAGE_SIZE_NAMES: Record<PageSize, string> = {
  A4: 'A4 (210×297mm)',
  A3: 'A3 (297×420mm)',
  B5: 'B5 (176×250mm)',
  ipad_pro: 'iPad Pro (160×233mm)'
}

/** 页面尺寸物理尺寸（毫米），供边距上限等计算复用，避免各处硬编码尺寸漂移 */
export const PAGE_SIZE_DIMS_MM: Record<PageSize, { w: number; h: number }> = {
  A4: { w: 210, h: 297 },
  A3: { w: 297, h: 420 },
  B5: { w: 176, h: 250 },
  ipad_pro: { w: 160, h: 233 }
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
