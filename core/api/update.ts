import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface UpdateInfo {
  current: string
  latest: string
  available: boolean
  notes: string
  assetUrl: string | null
  assetSize: number
  sha256: string | null
  publishedAt: string | null
  message: string | null
}

export interface UpdateProgress {
  downloaded: number
  total: number
}

export const checkUpdate = async (): Promise<UpdateInfo> => {
  return invoke<UpdateInfo>('check_update').catch((err) => {
    throw new Error(err?.toString() || '检查更新失败')
  })
}

export const downloadUpdate = async (
  url: string,
  version: string,
  sha256: string | null
): Promise<string> => {
  return invoke<string>('download_update', { url, version, sha256 }).catch((err) => {
    throw new Error(err?.toString() || '下载更新失败')
  })
}

export const applyUpdate = async (path: string): Promise<void> => {
  return invoke<void>('apply_update', { path })
}

export const onUpdateProgress = (cb: (p: UpdateProgress) => void): Promise<UnlistenFn> => {
  return listen<UpdateProgress>('update-progress', (event) => cb(event.payload))
}
