import { invoke } from '@tauri-apps/api/core'
import type { ExternalToolStatus, ImportResponse, MediaInfoStatus, VideoInfoImportResponse } from '../types/media'

export const importMedia = (paths: string[], recursive: boolean) => {
  return invoke<ImportResponse>('import_media', { paths, recursive })
}

export const getMediaInfoStatus = () => {
  return invoke<MediaInfoStatus>('get_mediainfo_status')
}

export const checkFfprobeStatus = () => {
  return invoke<ExternalToolStatus>('check_ffprobe_status')
}

export const checkExiftoolStatus = () => {
  return invoke<ExternalToolStatus>('check_exiftool_status')
}

export const importDetailedVideoInfo = (paths: string[], recursive: boolean) => {
  return invoke<VideoInfoImportResponse>('import_detailed_video_info', { paths, recursive })
}

export const getVideoRawXml = (path: string) => {
  return invoke<string | null>('get_video_raw_xml', { path })
}
