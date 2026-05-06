export interface WebdavConfig {
  enabled: boolean
  serverUrl: string
  username: string
  password: string
  autoSync?: boolean
}

export interface WebdavSyncStatus {
  enabled: boolean
  connected: boolean
  lastSyncVersion: number
  remoteVersion?: number
  lastUpdatedBy?: string
  lastUpdatedAt?: number
  pollingActive: boolean
}

export interface SyncConflict {
  entryId: string
  entryTitle: string
  localUpdatedAt: number
  remoteUpdatedAt: number
  localUpdatedBy: string
  remoteUpdatedBy: string
}

export interface ChangeLogEntry {
  timestamp: number
  deviceId: string
  action: string
  entryId: string
  entryTitle: string
}

export interface SyncResult {
  pushed: number
  pulled: number
  conflicts: SyncConflict[]
  newVersion: number
}
