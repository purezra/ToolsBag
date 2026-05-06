import { invoke } from '@tauri-apps/api/core'
import type { ConvertReq, ConvertResp, EpubConvertReq, EpubConvertResp } from '../types/image'

export const listImages = (inputDir: string, recursive = true) => {
  return invoke<{ name: string; path: string; format: string; size: number }[]>('list_images_tool3', {
    inputDir,
    recursive
  })
}

export const convertImages = (req: ConvertReq) => {
  return invoke<ConvertResp>('convert_tool3', { req })
}

export const convertToEpub = (req: EpubConvertReq) => {
  return invoke<EpubConvertResp>('convert_to_epub', { req })
}

export const readTool3Note = () => {
  return invoke<string>('read_tool3_note')
}
