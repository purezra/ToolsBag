import { invoke } from '@tauri-apps/api/core'
import type { Account, CodebookConfig, VaultEntries, VaultStatus, DevicesPayload, VaultxPreviewResult } from '../types/codebook'

export const getVaultStatus = async () => {
  return invoke<VaultStatus>('codebook_status')
}

export const initVault = async (password: string, deviceName?: string) => {
  return invoke<VaultStatus>('codebook_init', { password, deviceName })
}

export const unlockVault = async (password: string) => {
  return invoke<VaultStatus>('codebook_unlock', { password })
}

export const lockVault = async () => {
  return invoke('codebook_lock')
}

export const listEntries = async () => {
  return invoke<VaultEntries>('codebook_list')
}

export const listDevices = async () => {
  return invoke<DevicesPayload>('codebook_list_devices')
}

export const saveEntry = async (entry: Account) => {
  return invoke<Account>('codebook_save', { entry })
}

export const deleteEntry = async (id: string) => {
  return invoke<VaultEntries>('codebook_delete', { id })
}

export const restoreEntry = async (id: string) => {
  return invoke<VaultEntries>('codebook_restore', { id })
}

export const saveConfig = async (config: CodebookConfig) => {
  return invoke<CodebookConfig>('codebook_save_config', { config })
}

export const changeMasterPassword = async (newPassword: string) => {
  return invoke<VaultStatus>('codebook_change_password', { newPassword })
}

export const revokeDevice = async (targetId: string) => {
  return invoke<DevicesPayload>('codebook_revoke_device', { targetId })
}

export const exportVaultx = async (outputDir?: string) => {
  return invoke<{ filePath: string }>('codebook_export_vaultx', { req: { outputDir } })
}

export const importVaultx = async (filePath: string) => {
  return invoke<VaultEntries>('codebook_import_vaultx', { req: { filePath } })
}

export const previewVaultx = async (filePath: string) => {
  return invoke<VaultxPreviewResult>('codebook_preview_vaultx', { req: { filePath } })
}
