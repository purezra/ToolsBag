export type MediaKind = 'video' | 'image'

export type BaseRow = {
  id: number
  name: string
  path: string
  size: number
  mediaType: MediaKind
  status: 'pending' | 'success' | 'error'
  reason?: string
}

export type VideoRow = BaseRow & {
  durationSec?: number
  width?: number
  height?: number
  bitrateMbps?: number
  codec?: string
  frameRate?: string
}

export type ImageRow = BaseRow & {
  width?: number
  height?: number
  device?: string
  takenAt?: string
  focalLength?: string
}

export type RenameFieldKey =
  | 'seq'
  | 'duration'
  | 'resolution'
  | 'bitrate'
  | 'frameRate'
  | 'device'
  | 'takenAt'
  | 'focal'
  | 'size'
  | 'custom'
  | 'filename'

export type RenameField = {
  key: RenameFieldKey
  label: string
  enabled: boolean
}

export type ImportStats = {
  total: number
  success: number
  failed: number
  formatCounts: { ext: string; count: number }[]
}

export type ImportResponse = {
  items: (VideoRow | ImageRow)[]
  stats: ImportStats
}

export type MediaInfoStatus = {
  available: boolean
  path?: string
}

export type ExternalToolStatus = {
  name: string
  available: boolean
  version?: string
  path?: string
}

// ==================== 视频信息展览 - 详细元数据类型 ====================

export type DisplayLevel = 'public' | 'beginner' | 'advanced' | 'professional'

export type GeneralInfo = {
  format: string
  formatLong: string
  fileSize: number
  fileSizeStr: string
  duration: string
  durationMs: number
  overallBitRate: string
  title: string
  encodedDate: string
  writingApplication: string
  codecId: string
  streamCount: number
  encodedLibrary: string
}

export type DetailedVideoStream = {
  index: number
  // 大众级
  width: number
  height: number
  displayAspectRatio: string
  frameRate: string
  // 入门级
  codec: string
  bitRate: string
  frameRateMode: string
  bitDepth: string
  hdrFormat: string
  scanType: string
  // 进阶级
  formatProfile: string
  chromaSubsampling: string
  colorSpace: string
  colorPrimaries: string
  transferCharacteristics: string
  matrixCoefficients: string
  streamSize: string
  bitsPerPixelFrame: string
  language: string
  // 专业级
  cabac: string
  formatSettingsRefFrames: string
  encodedLibrary: string
  encodedLibrarySettings: string
  codecId: string
  duration: string
  durationMs: number
}

export type DetailedAudioStream = {
  index: number
  // 大众级
  channels: string
  channelLayout: string
  sampleRate: string
  // 入门级
  codec: string
  bitRate: string
  bitRateMode: string
  isDefault: boolean
  // 进阶级
  language: string
  title: string
  streamSize: string
  formatProfile: string
  compressionMode: string
  duration: string
  durationMs: number
  codecId: string
}

export type DetailedTextStream = {
  index: number
  format: string
  codecId: string
  language: string
  title: string
  isDefault: boolean
}

export type DetailedVideoMeta = {
  general: GeneralInfo
  videoStreams: DetailedVideoStream[]
  audioStreams: DetailedAudioStream[]
  textStreams: DetailedTextStream[]
}

export type VideoInfoItem = {
  id: number
  name: string
  path: string
  size: number
  status: string
  reason?: string
  detail?: DetailedVideoMeta
}

export type VideoInfoImportResponse = {
  items: VideoInfoItem[]
  total: number
  success: number
  failed: number
}
