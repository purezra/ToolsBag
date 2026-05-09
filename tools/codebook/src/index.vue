<script setup lang="ts">
import { ref, reactive, onMounted, computed } from 'vue'
import { Plus, Download, Upload, Search, Setting, ArrowDown, Lock, Unlock, RefreshRight, Key as KeyIcon, Files, FolderOpened } from '@element-plus/icons-vue'
import { ElMessage, ElAlert } from 'element-plus'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import PasswordGenerator from './components/password-generator.vue'
import AccountForm from './components/account-form.vue'
import AccountTable from './components/account-table.vue'
import IdentityPresetDialog from './components/identity-preset-dialog.vue'
import WebdavSettings from './components/webdav-settings.vue'
import { useCodebook } from './hooks/useCodebook'
import { useSettings } from '@core/hooks/useSettings'
import type { Account, IdentityCategory, VaultxPreviewResult } from './types/codebook'

const { t } = useSettings()

const codebook = reactive(useCodebook())

const activeTab = ref<'list' | 'add'>('list')
const recycleTab = ref<'active' | 'recycle'>('active')
const editDialogVisible = ref(false)
const editingAccount = ref<Account | null>(null)
const presetDialogVisible = ref(false)
const changePwdDialogVisible = ref(false)
const webdavSettingsVisible = ref(false)

const masterPassword = ref('')
const confirmPassword = ref('')
const deviceName = ref('本机')
const newPassword = ref('')
const newPasswordConfirm = ref('')

const fileInput = ref<HTMLInputElement | null>(null)
const vaultxPreviewDialogVisible = ref(false)
const vaultxPreviewData = ref<VaultxPreviewResult | null>(null)
const pendingVaultxPath = ref('')
const vaultxDir = ref('')
const vaultxDirDialogVisible = ref(false)

const VAULTX_DIR_KEY = 'codebook_vaultx_dir'

const isLocked = computed(() => codebook.status.locked || !codebook.status.initialized)

onMounted(() => {
  codebook.bootstrap()
  // 加载保存的VaultX目录设置
  const savedDir = localStorage.getItem(VAULTX_DIR_KEY)
  if (savedDir) {
    vaultxDir.value = savedDir
  }
})

const handleSelectVaultxDir = async () => {
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: vaultxDir.value || undefined
    })
    if (selected && typeof selected === 'string') {
      vaultxDir.value = selected
      localStorage.setItem(VAULTX_DIR_KEY, selected)
      ElMessage.success(t('目录设置成功'))
    }
  } catch (e: any) {
    ElMessage.error(t('选择目录失败'))
  }
}

const handleClearVaultxDir = () => {
  vaultxDir.value = ''
  localStorage.removeItem(VAULTX_DIR_KEY)
  ElMessage.success(t('已清除目录设置'))
}

const handleGenerate = () => {
  codebook.generatePassword()
}

const handleAddPreset = async (category: IdentityCategory, value: string) => {
  await codebook.addIdentityPreset(category, value)
}

const handleRemovePreset = async (category: IdentityCategory, value: string) => {
  await codebook.removeIdentityPreset(category, value)
}

const handleExportPresets = () => {
  codebook.downloadPresetsJson()
  ElMessage.success(t('导出成功'))
}

const handleImportPresets = async (jsonStr: string) => {
  const count = await codebook.importPresetsFromJson(jsonStr)
  if (count >= 0) {
    ElMessage.success(t('导入成功') + `: ${count} ` + t('条预设'))
  } else {
    ElMessage.error(t('导入失败'))
  }
}

const handleSubmit = async (data: Omit<Account, 'id' | 'createdAt' | 'updatedAt' | 'deleted'>) => {
  await codebook.addAccount(data)
  ElMessage.success(t('账号已保存'))
  activeTab.value = 'list'
  codebook.generatedPassword = ''
}

const handleEdit = (account: Account) => {
  editingAccount.value = account
  editDialogVisible.value = true
}

const handleEditSubmit = async (data: Omit<Account, 'id' | 'createdAt' | 'updatedAt' | 'deleted'>) => {
  if (editingAccount.value) {
    await codebook.updateAccount(editingAccount.value.id, data)
    ElMessage.success(t('已更新'))
  }
  editDialogVisible.value = false
  editingAccount.value = null
}

const handleDelete = async (id: string) => {
  await codebook.deleteAccount(id)
}

const handleRestore = async (id: string) => {
  await codebook.restoreAccount(id)
}

const handleExportJson = () => {
  codebook.downloadJson()
  ElMessage.success(t('导出成功'))
}

const handleExportXlsx = async () => {
  try {
    await codebook.downloadXlsx()
    ElMessage.success(t('导出成功'))
  } catch (e) {
    ElMessage.error(t('导出失败'))
  }
}

const handleExportVaultx = async () => {
  try {
    // 使用用户设置的目录，否则使用当前目录
    const outputDir = vaultxDir.value || '.'
    const res = await codebook.exportVaultxFile(outputDir)
    const path = (res as any).filePath || (res as any).file_path || ''
    ElMessage.success(t('导出成功') + (path ? `: ${path}` : ''))
  } catch (e) {
    ElMessage.error(t('导出失败'))
  }
}

const handleImportClick = () => {
  fileInput.value?.click()
}

const handleImportVaultxClick = async () => {
  try {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: 'VaultX ZIP', extensions: ['zip'] }],
      defaultPath: vaultxDir.value || undefined
    })
    if (!selected || Array.isArray(selected)) return
    
    pendingVaultxPath.value = selected
    const preview = await codebook.previewVaultxFile(selected)
    vaultxPreviewData.value = preview
    vaultxPreviewDialogVisible.value = true
  } catch (e: any) {
    ElMessage.error(t('预览失败') + ': ' + (e?.message || e))
  }
}

const handleConfirmImportVaultx = async () => {
  try {
    await codebook.importVaultxFile(pendingVaultxPath.value)
    ElMessage.success(t('导入成功'))
    vaultxPreviewDialogVisible.value = false
    vaultxPreviewData.value = null
    pendingVaultxPath.value = ''
  } catch (e: any) {
    ElMessage.error(t('导入失败') + ': ' + (e?.message || e))
  }
}

const getDiffTypeLabel = (type: string) => {
  switch (type) {
    case 'added': return t('新增')
    case 'modified': return t('修改')
    case 'deleted': return t('删除')
    default: return type
  }
}

const getDiffTypeTagType = (type: string) => {
  switch (type) {
    case 'added': return 'success'
    case 'modified': return 'warning'
    case 'deleted': return 'danger'
    default: return 'info'
  }
}

const handleFileChange = async (event: Event) => {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) return

  const reader = new FileReader()
  reader.onload = async (e) => {
    const content = e.target?.result as string
    const count = await codebook.importFromJson(content)
    if (count >= 0) {
      ElMessage.success(t('导入成功') + `: ${count} ` + t('条记录'))
    } else {
      ElMessage.error(t('导入失败'))
    }
  }
  reader.readAsText(file)
  target.value = ''
}

const handleInitVault = async () => {
  if (!masterPassword.value.trim()) {
    ElMessage.error(t('请输入主密码'))
    return
  }
  if (masterPassword.value !== confirmPassword.value) {
    ElMessage.error(t('两次输入的主密码不一致'))
    return
  }
  try {
    await codebook.initializeVault(masterPassword.value, deviceName.value || undefined)
    ElMessage.success(t('保险库已初始化并加锁保护'))
  } catch {
    // Error already handled in useCodebook
  } finally {
    masterPassword.value = ''
    confirmPassword.value = ''
  }
}

const handleUnlock = async () => {
  if (!masterPassword.value.trim()) {
    ElMessage.error(t('请输入主密码'))
    return
  }
  try {
    await codebook.unlock(masterPassword.value)
    ElMessage.success(t('已解锁'))
  } catch {
    // Error already handled in useCodebook
  } finally {
    masterPassword.value = ''
  }
}

const handleLock = async () => {
  try {
    await codebook.lock()
    ElMessage.success(t('已锁定'))
  } catch {
    ElMessage.error(t('锁定失败'))
  }
}

const handleChangePassword = async () => {
  if (!newPassword.value.trim()) {
    ElMessage.error(t('请输入新主密码'))
    return
  }
  if (newPassword.value !== newPasswordConfirm.value) {
    ElMessage.error(t('两次输入不一致'))
    return
  }
  await codebook.rotateMasterPassword(newPassword.value)
  newPassword.value = ''
  newPasswordConfirm.value = ''
  changePwdDialogVisible.value = false
  ElMessage.success(t('主密码已更新（仅重包裹设备密钥）'))
}
</script>

<template>
  <div class="codebook-tool">
    <el-skeleton v-if="codebook.loading" animated :rows="6" style="padding: 16px" />

    <template v-else>
      <div v-if="isLocked" class="lock-panel">
        <div class="lock-screen">
          <!-- Lock icon with animation -->
          <div class="lock-icon-wrap" :class="{ 'is-shaking': codebook.busy }">
            <div class="lock-icon-circle">
              <el-icon :size="48" class="lock-icon">
                <Lock />
              </el-icon>
            </div>
            <div class="lock-pulse" />
          </div>

          <div class="lock-content">
            <h2 class="lock-title">{{ t('端到端加密保险库') }}</h2>
            <p class="lock-subtitle">{{ t('主密码仅用于派生主密钥，永不上传、永不落盘') }}</p>

            <div class="lock-status">
              <el-tag type="warning" effect="dark" v-if="!codebook.status.initialized">{{ t('首次初始化') }}</el-tag>
              <el-tag type="success" effect="dark" v-else>{{ t('待解锁') }}</el-tag>
            </div>

            <el-form label-position="top" class="lock-form">
              <div class="lock-input-group">
                <el-form-item :label="t('主密码')" class="lock-form-item">
                  <el-input
                    v-model="masterPassword"
                    type="password"
                    show-password
                    autocomplete="off"
                    size="large"
                    :placeholder="t('输入主密码')"
                    @keyup.enter="codebook.status.initialized ? handleUnlock() : undefined"
                  />
                </el-form-item>

                <template v-if="!codebook.status.initialized">
                  <el-form-item :label="t('确认主密码')" class="lock-form-item">
                    <el-input
                      v-model="confirmPassword"
                      type="password"
                      show-password
                      autocomplete="off"
                      size="large"
                      :placeholder="t('再次输入主密码')"
                    />
                  </el-form-item>
                  <el-form-item :label="t('设备名称（可选）')" class="lock-form-item">
                    <el-input v-model="deviceName" autocomplete="off" size="large" />
                  </el-form-item>
                </template>
              </div>

              <div class="lock-actions">
                <template v-if="!codebook.status.initialized">
                  <el-button
                    type="primary"
                    size="large"
                    :icon="KeyIcon"
                    :loading="codebook.busy"
                    class="lock-btn lock-btn--primary"
                    @click="handleInitVault"
                  >
                    {{ t('初始化并解锁') }}
                  </el-button>
                </template>
                <template v-else>
                  <el-button
                    type="primary"
                    size="large"
                    :icon="Unlock"
                    :loading="codebook.busy"
                    class="lock-btn lock-btn--primary"
                    @click="handleUnlock"
                  >
                    {{ t('解锁') }}
                  </el-button>
                  <el-button
                    size="large"
                    :icon="RefreshRight"
                    class="lock-btn"
                    @click="codebook.bootstrap()"
                  >
                    {{ t('重新检测') }}
                  </el-button>
                </template>
              </div>
            </el-form>

            <p class="lock-footer-hint">
              {{ t('三层密钥：主密码 -> MK -> DK -> 每条独立 FK，所有数据文件均为.enc') }}
            </p>
          </div>
        </div>
      </div>

      <div v-else class="tool-shell">
        <div class="tool-header">
          <div class="header-left">
            <el-input
              v-model="codebook.searchQuery"
              :prefix-icon="Search"
              :placeholder="t('搜索账号...')"
              clearable
              style="width: 280px"
            />
            <div class="meta">
              <el-tag size="small" type="success">{{ t('设备') }}: {{ codebook.status.deviceName || 'PC' }}</el-tag>
              <el-tag size="small" type="info">{{ t('版本') }}: {{ codebook.status.globalVersion }}</el-tag>
            </div>
          </div>
          <div class="header-right">
            <el-button :icon="Setting" @click="presetDialogVisible = true">{{ t('预设账号标识') }}</el-button>
            <el-button :icon="Setting" plain @click="webdavSettingsVisible = true">WebDAV</el-button>
        <el-dropdown trigger="click" @command="(cmd: string) => cmd === 'json' ? handleExportJson() : (cmd === 'xlsx' ? handleExportXlsx() : handleExportVaultx())">
          <el-button :icon="Download">
            {{ t('导出') }}<el-icon class="el-icon--right"><ArrowDown /></el-icon>
          </el-button>
          <template #dropdown>
            <el-dropdown-menu>
              <el-dropdown-item command="json">{{ t('导出 JSON') }}</el-dropdown-item>
              <el-dropdown-item command="xlsx">{{ t('导出 XLSX') }}</el-dropdown-item>
              <el-dropdown-item command="vaultx">
                <el-icon><Files /></el-icon>
                <span style="margin-left:6px;">{{ t('导出 VaultX (.vaultx)') }}</span>
              </el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
        <el-dropdown trigger="click" @command="(cmd: string) => cmd === 'json' ? handleImportClick() : handleImportVaultxClick()">
          <el-button :icon="Upload">
            {{ t('导入') }}<el-icon class="el-icon--right"><ArrowDown /></el-icon>
          </el-button>
          <template #dropdown>
            <el-dropdown-menu>
              <el-dropdown-item command="json">{{ t('导入 JSON') }}</el-dropdown-item>
              <el-dropdown-item command="vaultx">
                <el-icon><Files /></el-icon>
                <span style="margin-left:6px;">{{ t('导入 VaultX (.vaultx)') }}</span>
              </el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
            <el-tooltip :content="vaultxDir || t('点击设置VaultX目录')" placement="bottom">
              <el-button :icon="FolderOpened" @click="vaultxDirDialogVisible = true">
                {{ t('目录') }}
              </el-button>
            </el-tooltip>
            <input
              ref="fileInput"
              type="file"
              accept=".json"
              style="display: none"
              @change="handleFileChange"
            />
            <el-button :icon="KeyIcon" @click="changePwdDialogVisible = true">{{ t('修改主密码') }}</el-button>
            <el-button type="primary" :icon="Plus" @click="activeTab = 'add'">
              {{ t('添加账号') }}
            </el-button>
            <el-button type="warning" plain :icon="Lock" @click="handleLock">{{ t('锁定') }}</el-button>
          </div>
        </div>

        <div class="tool-body">
          <el-tabs v-model="activeTab" type="border-card">
            <el-tab-pane :label="t('账号列表')" name="list">
              <div class="list-stats">
                <span>{{ t('共') }} {{ codebook.filteredAccounts.length }} {{ t('条记录') }}</span>
                <el-tag v-if="codebook.recycleBin.length" type="warning" effect="plain">
                  {{ t('回收站') }}: {{ codebook.recycleBin.length }}
                </el-tag>
              </div>
              <el-tabs v-model="recycleTab" type="card" class="sub-tabs">
                <el-tab-pane :label="t('在用')" name="active">
                  <AccountTable
                    :accounts="codebook.filteredAccounts"
                    @edit="handleEdit"
                    @delete="handleDelete"
                  />
                </el-tab-pane>
                <el-tab-pane :label="t('回收站')" name="recycle">
                  <AccountTable
                    mode="recycle"
                    :accounts="codebook.filteredDeleted"
                    @restore="handleRestore"
                  />
                </el-tab-pane>
              </el-tabs>
            </el-tab-pane>

            <el-tab-pane :label="t('添加账号')" name="add">
              <el-row :gutter="24">
                <el-col :span="12">
                  <el-card shadow="never">
                    <template #header>
                      <span class="card-title">{{ t('密码生成器') }}</span>
                    </template>
                    <PasswordGenerator
                      :options="codebook.passwordOptions"
                      :generated="codebook.generatedPassword"
                      @update:options="(v) => Object.assign(codebook.passwordOptions, v)"
                      @generate="handleGenerate"
                    />
                  </el-card>
                </el-col>
                <el-col :span="12">
                  <el-card shadow="never">
                    <template #header>
                      <span class="card-title">{{ t('账号信息') }}</span>
                    </template>
                    <AccountForm
                      :config="codebook.config"
                      :generated-password="codebook.generatedPassword"
                      :all-names="codebook.allNames"
                      :all-identities="codebook.allIdentities"
                      @submit="handleSubmit"
                      @cancel="activeTab = 'list'"
                    />
                  </el-card>
                </el-col>
              </el-row>
            </el-tab-pane>
          </el-tabs>
        </div>

        <el-dialog
          v-model="editDialogVisible"
          :title="t('编辑账号')"
          width="500px"
          destroy-on-close
        >
          <AccountForm
            v-if="editingAccount"
            :config="codebook.config"
            :initial-data="editingAccount"
            :all-names="codebook.allNames"
            :all-identities="codebook.allIdentities"
            @submit="handleEditSubmit"
            @cancel="editDialogVisible = false"
          />
        </el-dialog>

        <IdentityPresetDialog
          v-model:visible="presetDialogVisible"
          :config="codebook.config"
          @add-preset="handleAddPreset"
          @remove-preset="handleRemovePreset"
          @export-presets="handleExportPresets"
          @import-presets="handleImportPresets"
        />

        <el-dialog
          v-model="webdavSettingsVisible"
          title="WebDAV 云同步"
          width="560px"
          destroy-on-close
        >
          <WebdavSettings @close="webdavSettingsVisible = false" />
        </el-dialog>

        <el-dialog
          v-model="changePwdDialogVisible"
          :title="t('修改主密码')"
          width="420px"
        >
          <el-form label-position="top">
            <el-form-item :label="t('新主密码')">
              <el-input v-model="newPassword" type="password" show-password autocomplete="off" />
            </el-form-item>
            <el-form-item :label="t('确认新主密码')">
              <el-input v-model="newPasswordConfirm" type="password" show-password autocomplete="off" />
            </el-form-item>
          </el-form>
          <template #footer>
            <el-button @click="changePwdDialogVisible = false">{{ t('取消') }}</el-button>
            <el-button type="primary" :loading="codebook.busy" @click="handleChangePassword">{{ t('保存') }}</el-button>
          </template>
        </el-dialog>

        <el-dialog
          v-model="vaultxPreviewDialogVisible"
          :title="t('导入 VaultX 预览')"
          width="600px"
        >
          <template v-if="vaultxPreviewData">
            <div class="vaultx-preview">
              <div class="preview-summary">
                <el-descriptions :column="2" border size="small">
                  <el-descriptions-item :label="t('本地账号数')">{{ vaultxPreviewData.localCount }}</el-descriptions-item>
                  <el-descriptions-item :label="t('导入文件账号数')">{{ vaultxPreviewData.importCount }}</el-descriptions-item>
                </el-descriptions>
              </div>

              <el-alert
                v-if="vaultxPreviewData.isSame"
                type="info"
                :title="t('当前内容相同，是否覆盖？')"
                :closable="false"
                show-icon
                class="preview-alert"
              />

              <template v-else>
                <el-alert
                  v-if="vaultxPreviewData.configChanged"
                  type="warning"
                  :title="t('预设账号标识或标签有变化')"
                  :closable="false"
                  show-icon
                  class="preview-alert"
                />

                <div v-if="vaultxPreviewData.diffs.length > 0" class="diff-section">
                  <p class="diff-title">{{ t('当前和本地账号数据差别') }}：</p>
                  <el-table :data="vaultxPreviewData.diffs" max-height="300" size="small" stripe>
                    <el-table-column prop="name" :label="t('名称')" min-width="120" />
                    <el-table-column prop="accountIdentity" :label="t('账号')" min-width="150" />
                    <el-table-column :label="t('变更类型')" width="100">
                      <template #default="{ row }">
                        <el-tag :type="getDiffTypeTagType(row.diffType)" size="small">
                          {{ getDiffTypeLabel(row.diffType) }}
                        </el-tag>
                      </template>
                    </el-table-column>
                    <el-table-column prop="details" :label="t('详情')" min-width="150" show-overflow-tooltip />
                  </el-table>
                </div>
              </template>
            </div>
          </template>
          <template #footer>
            <el-button @click="vaultxPreviewDialogVisible = false">{{ t('取消') }}</el-button>
            <el-button type="primary" @click="handleConfirmImportVaultx">{{ t('确认导入') }}</el-button>
          </template>
        </el-dialog>

        <el-dialog
          v-model="vaultxDirDialogVisible"
          :title="t('VaultX 导入导出目录')"
          width="500px"
        >
          <div class="vaultx-dir-setting">
            <el-alert
              type="info"
              :closable="false"
              show-icon
              class="mb16"
            >
              <template #title>
                {{ t('设置VaultX文件的默认导入导出目录，方便快速定位备份文件') }}
              </template>
            </el-alert>

            <div class="dir-display">
              <el-input
                v-model="vaultxDir"
                :placeholder="t('未设置，使用当前目录')"
                readonly
              >
                <template #prepend>
                  <el-icon><FolderOpened /></el-icon>
                </template>
              </el-input>
            </div>

            <div class="dir-actions">
              <el-button type="primary" @click="handleSelectVaultxDir">
                {{ t('选择目录') }}
              </el-button>
              <el-button v-if="vaultxDir" @click="handleClearVaultxDir">
                {{ t('清除设置') }}
              </el-button>
            </div>
          </div>
          <template #footer>
            <el-button @click="vaultxDirDialogVisible = false">{{ t('关闭') }}</el-button>
          </template>
        </el-dialog>
      </div>
    </template>
  </div>
</template>

<style scoped>
.codebook-tool {
  height: 100%;
  display: flex;
  flex-direction: column;
}
.tool-shell {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.tool-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px;
  background: var(--el-bg-color);
  border-bottom: 1px solid var(--el-border-color-lighter);
}
.header-left {
  display: flex;
  gap: 12px;
  align-items: center;
}
.header-right {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  justify-content: flex-end;
}
.meta {
  display: flex;
  gap: 8px;
}
.tool-body {
  flex: 1;
  padding: 16px;
  overflow: auto;
}
.list-stats {
  margin-bottom: 12px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  display: flex;
  gap: 8px;
  align-items: center;
}
.card-title {
  font-weight: 600;
}
/* Full-screen lock panel */
.lock-panel {
  position: absolute;
  inset: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-page, #f5f6fa);
  overflow: auto;
}

[data-theme='dark'] .lock-panel {
  background: radial-gradient(circle at 50% 30%, rgba(40, 50, 70, 0.95), rgba(15, 17, 23, 0.98));
}

.lock-screen {
  display: flex;
  flex-direction: column;
  align-items: center;
  width: 100%;
  max-width: 440px;
  padding: 40px 32px;
}

/* Lock icon */
.lock-icon-wrap {
  position: relative;
  margin-bottom: 32px;
}

.lock-icon-circle {
  width: 88px;
  height: 88px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  background: var(--accent-gradient);
  color: var(--text-inverse, #fff);
  box-shadow: 0 12px 40px rgba(79, 139, 255, 0.3);
  position: relative;
  z-index: 1;
}

.lock-icon {
  animation: lock-bob 3s ease-in-out infinite;
}

@keyframes lock-bob {
  0%, 100% { transform: translateY(0); }
  50% { transform: translateY(-4px); }
}

.lock-pulse {
  position: absolute;
  inset: -12px;
  border-radius: 50%;
  border: 2px solid var(--accent, #4f8bff);
  opacity: 0;
  animation: lock-pulse 3s ease-in-out infinite;
}

@keyframes lock-pulse {
  0% { opacity: 0; transform: scale(0.8); }
  50% { opacity: 0.3; transform: scale(1); }
  100% { opacity: 0; transform: scale(1.2); }
}

.lock-icon-wrap.is-shaking .lock-icon {
  animation: lock-shake 0.4s ease-in-out;
}

@keyframes lock-shake {
  0%, 100% { transform: translateX(0) rotate(0); }
  20% { transform: translateX(-6px) rotate(-8deg); }
  40% { transform: translateX(6px) rotate(8deg); }
  60% { transform: translateX(-4px) rotate(-5deg); }
  80% { transform: translateX(4px) rotate(5deg); }
}

/* Content */
.lock-content {
  width: 100%;
  text-align: center;
}

.lock-title {
  margin: 0 0 8px;
  font-size: 24px;
  font-weight: 800;
  color: var(--text-primary);
  letter-spacing: -0.5px;
}

.lock-subtitle {
  margin: 0 0 20px;
  font-size: 14px;
  color: var(--text-secondary);
}

.lock-status {
  margin-bottom: 28px;
}

/* Form */
.lock-form {
  text-align: left;
}

.lock-input-group {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 20px;
}

.lock-form-item {
  margin-bottom: 0;
}

.lock-form-item :deep(.el-form-item__label) {
  font-weight: 600;
  color: var(--text-secondary);
}

/* Actions */
.lock-actions {
  display: flex;
  gap: 12px;
  justify-content: center;
  margin-bottom: 24px;
}

.lock-btn {
  min-width: 140px;
}

.lock-btn--primary {
  box-shadow: 0 8px 24px rgba(79, 139, 255, 0.3);
}

.lock-btn--primary:hover {
  box-shadow: 0 12px 32px rgba(79, 139, 255, 0.4);
  transform: translateY(-1px);
}

.lock-btn--primary:active {
  transform: translateY(0);
}

/* Footer hint */
.lock-footer-hint {
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.6;
  max-width: 360px;
  margin: 0 auto;
}

.mb16 {
  margin-bottom: 16px;
}
.sub-tabs {
  margin-top: 8px;
}
.vaultx-preview {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.preview-summary {
  margin-bottom: 8px;
}
.preview-alert {
  margin-bottom: 12px;
}
.diff-section {
  margin-top: 8px;
}
.diff-title {
  margin: 0 0 12px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}
.vaultx-dir-setting {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.dir-display {
  margin-bottom: 8px;
}
.dir-actions {
  display: flex;
  gap: 12px;
}
</style>

<style>
.codebook-tool .mono-text,
.codebook-tool .password-display :deep(.el-input__inner),
.codebook-tool .el-table .mono-text,
.codebook-tool input[type="password"],
.codebook-tool .preset-value,
.codebook-tool .preset-option {
  font-family: 'JetBrains Mono', 'Consolas', 'Monaco', monospace !important;
}
</style>
