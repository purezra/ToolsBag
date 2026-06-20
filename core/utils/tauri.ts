import { isTauri } from '@tauri-apps/api/core'

export const hasTauriRuntime = () => typeof window !== 'undefined' && isTauri()
