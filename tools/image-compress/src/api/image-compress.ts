import { invoke } from '@tauri-apps/api/core'
export { formatBytes as formatFileSize } from '@core/utils/format'

export type OutputFormat = 'auto' | 'jxl' | 'avif'
export type CompressMode = 'lossless' | 'near_lossless' | 'lossy'
export type EffortPreset = 'fast' | 'balanced' | 'best'
export type MetadataPolicy = 'keep' | 'strip'

export interface LibjxlStatus {
  available: boolean
  path?: string | null
  version?: string | null
  message: string
}

export interface CompressConfig {
  output_format: OutputFormat
  mode: CompressMode
  effort_preset: EffortPreset
  metadata_policy: MetadataPolicy
  advanced_mode: boolean
  quality: number
  /** JXL 有损档的 butteraugli distance（1.0=视觉无损，越大体积越小）。近无损档固定 1.0；无损档忽略。 */
  distance: number
  keep_exif: boolean
  max_dimension: number
  output_dir: string
  zip_output: boolean
  jxl_effort: number
  jxl_jpeg_lossless: boolean
  avif_color_quality: number
  avif_alpha_quality: number
  avif_speed: number
  keep_hdr: boolean
}

export interface CompressInput {
  path: string
  name: string
  ext: string
  width: number
  height: number
  file_size: number
  has_alpha: boolean
  has_exif: boolean
}

export interface CompressAnalysis {
  files: CompressInput[]
  skipped: string[]
  total_size: number
}

export interface CompressResult {
  input_path: string
  output_path?: string | null
  success: boolean
  original_size: number
  output_size: number
  compression_ratio: number
  error?: string | null
}

export interface CompressSummary {
  total: number
  success: number
  failed: number
  original_total_size: number
  output_total_size: number
  compression_ratio: number
  zip_path?: string | null
  zip_error?: string | null
  results: CompressResult[]
}

export interface CompressProgress {
  current: number
  total: number
  current_file: string
  phase: string
}

export async function getLibjxlStatus(): Promise<LibjxlStatus> {
  return await invoke('get_libjxl_status')
}

export async function analyzeCompressInputs(paths: string[], recursive = false): Promise<CompressAnalysis> {
  return await invoke('analyze_compress_inputs', { paths, recursive })
}

export async function compressImages(files: CompressInput[], config: CompressConfig): Promise<CompressSummary> {
  return await invoke('compress_images', { files, config })
}
