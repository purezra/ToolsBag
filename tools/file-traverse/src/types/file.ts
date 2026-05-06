export type TraverseReq = {
  inputDir: string
  outputDir?: string | null
  organizeByFormat: boolean
  minSizeMb?: number | null
  maxSizeMb?: number | null
  includePatterns: string[]
  excludePatterns: string[]
  /** 仅媒体模式 */
  mediaOnlyMode?: boolean
}

export type FormatStat = { ext: string; scanned: number; count: number; filtered: number }

/** 视频元数据 */
export type VideoMeta = {
  width: number
  height: number
  bitrate: string
  bitrateRaw: number
  aspectRatio: string
  frameRate: string
  frameRateMode: string
  codec: string
  hdrFormat?: string | null
  durationMs: number
  durationStr: string
}

/** 音频元数据 */
export type AudioMeta = {
  codec: string
  sampleRate: string
  bitrate: string
  durationMs: number
  durationStr: string
}

/** 图片元数据 */
export type ImageMeta = {
  width: number
  height: number
  format: string
}

/** 媒体文件信息 */
export type MediaFileInfo = {
  path: string
  name: string
  size: number
  mediaType: 'video' | 'audio' | 'image'
  newName: string
  video?: VideoMeta | null
  audio?: AudioMeta | null
  image?: ImageMeta | null
}

export type TraverseResp = {
  totalFiles: number
  copied: number
  skipped: number
  failed: number
  outputDir: string
  elapsedMs: number
  throughputBytes: number
  formatStats: FormatStat[]
  problems: { path: string; error: string }[]
  /** 媒体模式返回的媒体文件详情 */
  mediaFiles?: MediaFileInfo[]
}

export type PreviewStats = {
  files: { name: string; path: string; size: number }[]
  totalCount: number
  totalSize: number
}

/** 媒体预览请求 */
export type MediaPreviewReq = {
  inputDir: string
}

/** 媒体预览文件（带元数据） */
export type MediaPreviewFile = {
  path: string
  name: string
  ext: string
  size: number
  mediaType: 'video' | 'audio' | 'image'
  video?: VideoMeta | null
  audio?: AudioMeta | null
  image?: ImageMeta | null
}

/** 媒体预览结果 */
export type MediaPreviewResp = {
  videos: MediaPreviewFile[]
  audios: MediaPreviewFile[]
  images: MediaPreviewFile[]
}
