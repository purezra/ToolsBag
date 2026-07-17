import { invoke } from '@tauri-apps/api/core'
import type { Account } from '../types/codebook'
import type { WebdavConfig, WebdavSyncStatus, SyncConflict, ChangeLogEntry, SyncResult } from '../types/webdav'

export const saveWebdavConfig = async (config: WebdavConfig) => {
  return invoke<void>('webdav_save_config', { config })
}

export const loadWebdavConfig = async () => {
  return invoke<WebdavConfig | null>('webdav_load_config')
}

export const testWebdavConnection = async (config: WebdavConfig) => {
  return invoke<void>('webdav_test_connection', { config })
}

export const getWebdavStatus = async () => {
  return invoke<WebdavSyncStatus>('webdav_get_status')
}

export const startWebdavSync = async () => {
  return invoke<SyncResult>('webdav_start_sync')
}

export const syncEntryToRemote = async (entry: Account) => {
  return invoke<void>('webdav_sync_entry', { entry })
}

export const pullWebdavChanges = async () => {
  return invoke<SyncConflict[]>('webdav_pull_changes')
}

export const getWebdavChangelog = async () => {
  return invoke<ChangeLogEntry[]>('webdav_get_changelog')
}

export const startWebdavPolling = async () => {
  return invoke<void>('webdav_start_polling')
}

export const stopWebdavPolling = async () => {
  return invoke<void>('webdav_stop_polling')
}
