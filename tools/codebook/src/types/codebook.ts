export interface Account {
  id: string
  name: string
  accountIdentity: string
  password: string
  tags: string[]
  notes: string
  url: string
  images: string[]
  createdAt: string
  updatedAt: string
  deleted: boolean
}

export type IdentityCategory = 'email' | 'phone' | 'id'

export interface IdentityPreset {
  category: IdentityCategory
  value: string
}

export interface CodebookConfig {
  userIdentities: string[]
  identityPresets: IdentityPreset[]
  serviceTags: string[]
}

export interface PasswordOptions {
  length: number
  uppercase: boolean
  lowercase: boolean
  numbers: boolean
  symbols: boolean
  excludeAmbiguous: boolean
  excludeCodeSymbols: boolean
  customExclude: string
}

export interface VaultStatus {
  initialized: boolean
  locked: boolean
  deviceId?: string
  deviceName?: string
  globalVersion: number
  kdf?: KdfParams
}

export interface KdfParams {
  algorithm: string
  salt: string
  mem_cost: number
  time_cost: number
  parallelism: number
}

export interface VaultEntries {
  active: Account[]
  deleted: Account[]
  config: CodebookConfig
  globalVersion: number
}

export interface DeviceSummary {
  id: string
  name: string
  public_key: string
  added_at: string
  revoked_at?: string
  revoked: boolean
}

export interface DevicesPayload {
  current: string
  devices: DeviceSummary[]
}

export interface VaultxExportResult {
  filePath: string
}

export interface AccountDiff {
  name: string
  accountIdentity: string
  diffType: 'added' | 'modified' | 'deleted'
  details?: string
}

export interface VaultxPreviewResult {
  isSame: boolean
  localCount: number
  importCount: number
  diffs: AccountDiff[]
  configChanged: boolean
}
