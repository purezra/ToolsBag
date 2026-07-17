import { invoke } from '@tauri-apps/api/core'
import type { ImportResponse, MediaInfoStatus, VideoInfoImportResponse } from '../types/media'

export type MediaPerformanceRecord = {
  mode: 'light' | 'detailed'
  inputCount: number
  total: number
  success: number
  failed: number
  metadataMs: number
  displayMs: number
  totalMs: number
  cached: boolean
}

export const importMedia = (paths: string[], recursive: boolean) => {
  return invoke<ImportResponse>('import_media', { paths, recursive })
}

export const getMediaInfoStatus = () => {
  return invoke<MediaInfoStatus>('get_mediainfo_status')
}

export const importDetailedVideoInfo = (paths: string[], recursive: boolean) => {
  return invoke<VideoInfoImportResponse>('import_detailed_video_info', { paths, recursive })
}

export const recordMediaPerformance = (record: MediaPerformanceRecord) => {
  return invoke<string>('record_media_performance', { record })
}

export const getVideoRawXml = (path: string) => {
  return invoke<string | null>('get_video_raw_xml', { path })
}

export const getVideoCompleteInfo = (path: string) => {
  return invoke<string | null>('get_video_complete_info', { path })
}

export const getVideoXmlJson = (path: string) => {
  return invoke<string | null>('get_video_xml_json', { path })
}

export const getVideoXmlMarkdown = (path: string) => {
  return invoke<string | null>('get_video_xml_markdown', { path })
}
