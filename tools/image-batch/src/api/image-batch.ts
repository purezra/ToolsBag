import { invoke } from '@tauri-apps/api/core'
import type { EpubConvertReq, EpubConvertResp } from '../types/image'

export const listImages = (inputDir: string, recursive = true) => {
  return invoke<{ name: string; path: string; format: string; size: number }[]>('list_images', {
    inputDir,
    recursive
  })
}

export const convertToEpub = (req: EpubConvertReq) => {
  return invoke<EpubConvertResp>('convert_to_epub', { req })
}
