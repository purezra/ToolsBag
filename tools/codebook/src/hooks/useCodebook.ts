import { computed, ref, watch } from 'vue'
import {
  getVaultStatus,
  initVault,
  unlockVault,
  lockVault,
  listEntries,
  saveEntry,
  deleteEntry,
  restoreEntry,
  saveConfig,
  changeMasterPassword,
  exportVaultx,
  importVaultx,
  previewVaultx
} from '../api/codebook'
import type {
  Account,
  CodebookConfig,
  IdentityCategory,
  PasswordOptions,
  VaultEntries,
  VaultStatus,
  VaultxExportResult,
  VaultxPreviewResult
} from '../types/codebook'

const defaultConfig: CodebookConfig = {
  userIdentities: [],
  identityPresets: [],
  serviceTags: ['机场后台', '邮箱类APP', '手机APP', 'PC APP']
}

const defaultPasswordOptions: PasswordOptions = {
  length: 10,
  uppercase: true,
  lowercase: true,
  numbers: true,
  symbols: true,
  excludeAmbiguous: true,
  excludeCodeSymbols: true,
  customExclude: ''
}

export const useCodebook = () => {
  const status = ref<VaultStatus>({ initialized: false, locked: true, globalVersion: 0 })
  const loading = ref(false)
  const busy = ref(false)
  const error = ref<string | null>(null)

  const config = ref<CodebookConfig>({ ...defaultConfig })
  const accounts = ref<Account[]>([])
  const recycleBin = ref<Account[]>([])

  const showAddDialog = ref(false)
  const showEditDialog = ref(false)
  const searchQuery = ref('')
  const passwordOptions = ref<PasswordOptions>({ ...defaultPasswordOptions })
  const generatedPassword = ref('')

  const applyEntries = (data: VaultEntries) => {
    accounts.value = data.active || []
    recycleBin.value = data.deleted || []
    config.value = data.config || { ...defaultConfig }
    status.value.globalVersion = data.globalVersion || status.value.globalVersion
  }

  const bootstrap = async () => {
    loading.value = true
    try {
      status.value = await getVaultStatus()
      if (status.value.initialized && !status.value.locked) {
        const data = await listEntries()
        applyEntries(data)
      } else {
        accounts.value = []
        recycleBin.value = []
      }
    } catch (e: any) {
      error.value = e?.message || '加载状态失败'
    } finally {
      loading.value = false
    }
  }

  const initializeVault = async (password: string, deviceName?: string) => {
    busy.value = true
    error.value = null
    try {
      status.value = await initVault(password, deviceName)
      const data = await listEntries()
      applyEntries(data)
    } catch (e: any) {
      error.value = e?.message || '初始化失败'
      throw e
    } finally {
      busy.value = false
    }
  }

  const unlock = async (password: string) => {
    busy.value = true
    error.value = null
    try {
      status.value = await unlockVault(password)
      const data = await listEntries()
      applyEntries(data)
    } catch (e: any) {
      error.value = e?.message || '解锁失败'
      throw e
    } finally {
      busy.value = false
    }
  }

  const lock = async () => {
    try {
      await lockVault()
    } catch (e: any) {
      error.value = e?.message || '锁定失败'
      throw e
    }
    status.value.locked = true
    accounts.value = []
    recycleBin.value = []
  }

  const refreshEntries = async () => {
    const data = await listEntries()
    applyEntries(data)
  }

  // Password Generation
  const generatePassword = (): string => {
    // Guard for environments without crypto (avoid crashing render)
    const cryptoObj = (globalThis as any).crypto
    if (!cryptoObj || typeof cryptoObj.getRandomValues !== 'function') {
      generatedPassword.value = ''
      return ''
    }
    const opts = passwordOptions.value
    const ambiguous = '1lL0O'
    const codeSymbols = '{}[]()/\\\''
    const symbolSet = '!@#$%^&*'

    const filterExcluded = (str: string): string => {
      let result = str
      if (opts.excludeAmbiguous) {
        result = result.split('').filter((c) => !ambiguous.includes(c)).join('')
      }
      if (opts.excludeCodeSymbols) {
        result = result.split('').filter((c) => !codeSymbols.includes(c)).join('')
      }
      if (opts.customExclude) {
        const customChars = opts.customExclude.split('')
        result = result.split('').filter((c) => !customChars.includes(c)).join('')
      }
      return result
    }

    const charSets: string[] = []
    if (opts.uppercase) {
      const filtered = filterExcluded('ABCDEFGHIJKLMNOPQRSTUVWXYZ')
      if (filtered) charSets.push(filtered)
    }
    if (opts.lowercase) {
      const filtered = filterExcluded('abcdefghijklmnopqrstuvwxyz')
      if (filtered) charSets.push(filtered)
    }
    if (opts.numbers) {
      const filtered = filterExcluded('0123456789')
      if (filtered) charSets.push(filtered)
    }
    if (opts.symbols) {
      const filtered = filterExcluded(symbolSet)
      if (filtered) charSets.push(filtered)
    }

    if (charSets.length === 0) {
      generatedPassword.value = ''
      return ''
    }

    const allChars = charSets.join('')
    const resultChars: string[] = []
    const nonSymbolPool = [
      filterExcluded('ABCDEFGHIJKLMNOPQRSTUVWXYZ'),
      filterExcluded('abcdefghijklmnopqrstuvwxyz'),
      filterExcluded('0123456789')
    ]
      .filter((s) => s.length > 0)
      .join('')

    // ensure we have non-symbol characters if we need to avoid symbols in first 3
    if (nonSymbolPool.length === 0) {
      generatedPassword.value = ''
      return ''
    }

    const randomArray = new Uint32Array(opts.length + charSets.length)
    cryptoObj.getRandomValues(randomArray)
    let randomIndex = 0

    for (const set of charSets) {
      const idx = randomArray[randomIndex++]! % set.length
      resultChars.push(set[idx]!)
    }

    const remaining = opts.length - charSets.length
    for (let i = 0; i < remaining; i++) {
      const idx = randomArray[randomIndex++]! % allChars.length
      resultChars.push(allChars[idx]!)
    }

    const shuffleArray = new Uint32Array(resultChars.length)
    cryptoObj.getRandomValues(shuffleArray)
    for (let i = resultChars.length - 1; i > 0; i--) {
      const j = shuffleArray[i]! % (i + 1)
      ;[resultChars[i], resultChars[j]] = [resultChars[j]!, resultChars[i]!]
    }

    // ensure first 3 chars are not symbols
    const maxHead = Math.min(3, resultChars.length)
    for (let i = 0; i < maxHead; i++) {
      if (symbolSet.includes(resultChars[i]!)) {
        const idx = randomArray[(randomIndex + i) % randomArray.length]! % nonSymbolPool.length
        resultChars[i] = nonSymbolPool[idx]!
      }
    }

    // ensure at least one symbol remains if symbols are requested and length allows placing after index 2
    if (opts.symbols) {
      const hasSymbolBeyondHead = resultChars.some((c, idx) => idx >= 3 && symbolSet.includes(c))
      if (!hasSymbolBeyondHead) {
        if (resultChars.length <= 3) {
          generatedPassword.value = ''
          return ''
        }
        const idx = 3 + (randomArray[(randomIndex + 7) % randomArray.length]! % (resultChars.length - 3))
        const symIdx = randomArray[(randomIndex + 9) % randomArray.length]! % symbolSet.length
        resultChars[idx] = symbolSet[symIdx]!
      }
    }

    const result = resultChars.join('')
    generatedPassword.value = result
    return result
  }

  // Watch password options and regenerate - must be after generatePassword definition
  watch(passwordOptions, () => {
    generatePassword()
  }, { deep: true, immediate: true })

  const buildAccountPayload = (
    data: Omit<Account, 'id' | 'createdAt' | 'updatedAt' | 'deleted'>,
    existing?: Account
  ): Account => {
    return {
      id: existing?.id || '',
      name: data.name.trim(),
      accountIdentity: data.accountIdentity.trim(),
      password: data.password,
      tags: data.tags || [],
      notes: data.notes || '',
      url: data.url || '',
      images: data.images || [],
      createdAt: existing?.createdAt || '',
      updatedAt: existing?.updatedAt || '',
      deleted: existing?.deleted ?? false
    }
  }

  const addAccount = async (data: Omit<Account, 'id' | 'createdAt' | 'updatedAt' | 'deleted'>) => {
    const payload = buildAccountPayload(data)
    await saveEntry(payload)
    await refreshEntries()
  }

  const updateAccount = async (id: string, updates: Omit<Account, 'id' | 'createdAt' | 'updatedAt' | 'deleted'>) => {
    const existing = accounts.value.find((a) => a.id === id)
    const payload = buildAccountPayload(updates, existing)
    payload.id = id
    await saveEntry(payload)
    await refreshEntries()
  }

  const deleteAccount = async (id: string) => {
    const data = await deleteEntry(id)
    applyEntries(data)
  }

  const restoreAccount = async (id: string) => {
    const data = await restoreEntry(id)
    applyEntries(data)
  }

  const filteredAccounts = computed(() => {
    if (!searchQuery.value) return accounts.value
    const query = searchQuery.value.toLowerCase()
    return accounts.value.filter(
      (a) =>
        a.name.toLowerCase().includes(query) ||
        a.accountIdentity.toLowerCase().includes(query) ||
        a.tags.some((t) => t.toLowerCase().includes(query)) ||
        a.notes.toLowerCase().includes(query)
    )
  })

  const filteredDeleted = computed(() => {
    if (!searchQuery.value) return recycleBin.value
    const query = searchQuery.value.toLowerCase()
    return recycleBin.value.filter(
      (a) =>
        a.name.toLowerCase().includes(query) ||
        a.accountIdentity.toLowerCase().includes(query) ||
        a.tags.some((t) => t.toLowerCase().includes(query)) ||
        a.notes.toLowerCase().includes(query)
    )
  })

  // Config helpers
  const persistConfig = async () => {
    const saved = await saveConfig(config.value)
    config.value = saved
  }

  const addIdentityPreset = async (category: IdentityCategory, value: string) => {
    if (!value.trim()) return
    const exists = config.value.identityPresets.some((p) => p.category === category && p.value === value)
    if (!exists) {
      config.value.identityPresets.push({ category, value: value.trim() })
      await persistConfig()
    }
  }

  const removeIdentityPreset = async (category: IdentityCategory, value: string) => {
    config.value.identityPresets = config.value.identityPresets.filter(
      (p) => !(p.category === category && p.value === value)
    )
    await persistConfig()
  }

  const getPresetsByCategory = (category: IdentityCategory): string[] => {
    return config.value.identityPresets.filter((p) => p.category === category).map((p) => p.value)
  }

  const allIdentities = computed(() => {
    const fromPresets = config.value.identityPresets.map((p) => p.value)
    const fromAccounts = accounts.value.map((a) => a.accountIdentity).filter(Boolean)
    return [...new Set([...fromPresets, ...fromAccounts])]
  })

  const allNames = computed(() => {
    return [...new Set(accounts.value.map((a) => a.name).filter(Boolean))]
  })

  const exportAsJson = () => {
    const data = accounts.value.map(({ images, deleted, updatedAt, ...rest }) => rest)
    return JSON.stringify(data, null, 2)
  }

  const downloadJson = () => {
    const json = exportAsJson()
    const blob = new Blob([json], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `codebook_export_${new Date().toISOString().slice(0, 10)}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  // XLSX export uses runtime import to avoid bundling heavy dep
  const downloadXlsx = async () => {
    const ExcelJS = await import('exceljs')
    const workbook = new ExcelJS.Workbook()
    const worksheet = workbook.addWorksheet('Accounts')
    worksheet.columns = [
      { header: '名称', key: 'name', width: 20 },
      { header: '账号', key: 'accountIdentity', width: 25 },
      { header: '密码', key: 'password', width: 25 },
      { header: 'APP/服务', key: 'tags', width: 20 },
      { header: '备注', key: 'notes', width: 30 },
      { header: '链接', key: 'url', width: 30 },
      { header: '附件', key: 'images', width: 40 }
    ]
    const headerRow = worksheet.getRow(1)
    headerRow.font = { bold: true }
    headerRow.fill = { type: 'pattern', pattern: 'solid', fgColor: { argb: 'FFE0E0E0' } }

    for (let i = 0; i < accounts.value.length; i++) {
      const account = accounts.value[i]!
      const rowNum = i + 2

      worksheet.addRow({
        name: account.name,
        accountIdentity: account.accountIdentity,
        password: account.password,
        tags: account.tags.join(', '),
        notes: account.notes,
        url: account.url,
        images: ''
      })

      if (account.images && account.images.length > 0) {
        const row = worksheet.getRow(rowNum)
        row.height = 60
        for (let j = 0; j < account.images.length; j++) {
          const base64 = account.images[j]!
          if (base64.startsWith('data:image')) {
            const base64Data = base64.split(',')[1]
            if (base64Data) {
              const ext = base64.includes('png') ? 'png' : 'jpeg'
              const imageId = workbook.addImage({ base64: base64Data, extension: ext })
              worksheet.addImage(imageId, {
                tl: { col: 6 + j * 0.8, row: rowNum - 1 },
                ext: { width: 50, height: 50 }
              })
            }
          }
        }
      }
    }

    const buffer = await workbook.xlsx.writeBuffer()
    const blob = new Blob([buffer], {
      type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
    })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `codebook_export_${new Date().toISOString().slice(0, 10)}.xlsx`
    a.click()
    URL.revokeObjectURL(url)
  }

  const exportPresetsJson = () => {
    return JSON.stringify(config.value.identityPresets, null, 2)
  }

  const downloadPresetsJson = () => {
    const json = exportPresetsJson()
    const blob = new Blob([json], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `codebook_presets_${new Date().toISOString().slice(0, 10)}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  const importPresetsFromJson = async (jsonStr: string): Promise<number> => {
    try {
      const data = JSON.parse(jsonStr) as { category: string; value: string }[]
      if (!Array.isArray(data)) return -1
      let imported = 0
      data.forEach((item) => {
        if (item.category && item.value && ['email', 'phone', 'id'].includes(item.category)) {
          const exists = config.value.identityPresets.some(
            (p) => p.category === item.category && p.value === item.value
          )
          if (!exists) {
            config.value.identityPresets.push({
              category: item.category as IdentityCategory,
              value: item.value
            })
            imported++
          }
        }
      })
      if (imported > 0) {
        await persistConfig()
      }
      return imported
    } catch {
      return -1
    }
  }

  const importFromJson = async (jsonStr: string): Promise<number> => {
    try {
      const data = JSON.parse(jsonStr) as Partial<Account>[]
      let imported = 0
      for (const item of data) {
        if (item.name && item.password) {
          const identity = (item as any).accountIdentity || (item as any).account_identity || ''
          const exists = accounts.value.some(
            (a) => a.name === item.name && a.accountIdentity === identity
          )
          if (!exists) {
            const payload: Account = {
              id: item.id || '',
              name: item.name,
              accountIdentity: identity,
              password: item.password,
              tags: item.tags || [],
              notes: item.notes || '',
              url: item.url || '',
              images: item.images || [],
              createdAt: (item as any).createdAt || (item as any).created_at || '',
              updatedAt: (item as any).updatedAt || (item as any).updated_at || '',
              deleted: !!(item as any).deleted
            }
            await saveEntry(payload)
            imported++
          }
        }
      }
      if (imported > 0) {
        await refreshEntries()
      }
      return imported
    } catch {
      return -1
    }
  }

  const rotateMasterPassword = async (newPassword: string) => {
    status.value = await changeMasterPassword(newPassword)
  }

  const exportVaultxFile = async (outputDir?: string): Promise<VaultxExportResult> => {
    const res = await exportVaultx(outputDir)
    return { filePath: (res as any).file_path || (res as any).filePath }
  }

  const previewVaultxFile = async (filePath: string): Promise<VaultxPreviewResult> => {
    return await previewVaultx(filePath)
  }

  const importVaultxFile = async (filePath: string): Promise<void> => {
    const data = await importVaultx(filePath)
    applyEntries(data)
  }

  return {
    status,
    loading,
    busy,
    error,
    config,
    accounts,
    recycleBin,
    filteredAccounts,
    filteredDeleted,
    showAddDialog,
    showEditDialog,
    searchQuery,
    passwordOptions,
    generatedPassword,
    allIdentities,
    allNames,
    // lifecycle
    bootstrap,
    initializeVault,
    unlock,
    lock,
    refreshEntries,
    // actions
    generatePassword,
    addAccount,
    updateAccount,
    deleteAccount,
    restoreAccount,
    exportAsJson,
    downloadJson,
    downloadXlsx,
    importFromJson,
    addIdentityPreset,
    removeIdentityPreset,
    getPresetsByCategory,
    exportPresetsJson,
    downloadPresetsJson,
    importPresetsFromJson,
    rotateMasterPassword,
    exportVaultxFile,
    previewVaultxFile,
    importVaultxFile
  }
}
