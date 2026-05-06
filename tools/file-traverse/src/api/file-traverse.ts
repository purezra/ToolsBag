import { invoke } from '@tauri-apps/api/core'
import type { TraverseReq, TraverseResp, MediaPreviewReq, MediaPreviewResp } from '../types/file'

export const traverseCopy = (req: TraverseReq) => {
  return invoke<TraverseResp>('traverse_copy', { req })
}

export const checkMediaInfo = () => {
  return invoke<boolean>('check_mediainfo')
}

export const previewMedia = (req: MediaPreviewReq) => {
  return invoke<MediaPreviewResp>('preview_media', { req })
}
