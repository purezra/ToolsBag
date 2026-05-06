import { invoke } from '@tauri-apps/api/core'

export type PickKind = 'file' | 'folder' | 'clipboard'

export const selectPaths = async (kind: 'file' | 'folder'): Promise<string[]> => {
  const list = await invoke<string[]>('select_media_paths', { kind }).catch((err) => {
    throw new Error(err?.toString() || '选择路径失败')
  })
  return list || []
}

export const readClipboardPaths = async (): Promise<string[]> => {
  const list = await invoke<string[]>('read_clipboard_paths').catch((err) => {
    throw new Error(err?.toString() || '读取剪贴板失败')
  })
  return list || []
}

export const openParentDir = async (path: string) => {
  return invoke('open_parent_dir', { path })
}

export type FileEntry = {
  path: string
  name?: string
  children?: FileEntry[]
}
