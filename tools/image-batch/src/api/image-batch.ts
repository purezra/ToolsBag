import { invoke } from '@tauri-apps/api/core'
import type { ConvertReq, ConvertResp, EpubConvertReq, EpubConvertResp } from '../types/image'

export const listImages = (inputDir: string, recursive = true) => {
  return invoke<{ name: string; path: string; format: string; size: number }[]>('list_images', {
    inputDir,
    recursive
  })
}

export const convertImages = (req: ConvertReq) => {
  return invoke<ConvertResp>('convert_image_batch', { req })
}

export const convertToEpub = (req: EpubConvertReq) => {
  return invoke<EpubConvertResp>('convert_to_epub', { req })
}

export const readImageBatchNote = () => {
  return invoke<string>('read_image_batch_note')
}
