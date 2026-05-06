<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { View, Hide, Plus, ArrowDown, Delete, Picture } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import type { Account, CodebookConfig, IdentityCategory } from '@codebook/types/codebook'
import { useSettings } from '@core/hooks/useSettings'

const props = defineProps<{
  config: CodebookConfig
  initialData?: Partial<Account>
  generatedPassword?: string
  allNames?: string[]
  allIdentities?: string[]
}>()

const emit = defineEmits<{
  submit: [data: Omit<Account, 'id' | 'createdAt' | 'updatedAt' | 'deleted'>]
  cancel: []
}>()

const { t } = useSettings()

const form = ref({
  name: props.initialData?.name || '',
  accountIdentity: props.initialData?.accountIdentity || '',
  password: props.initialData?.password || '',
  tags: props.initialData?.tags || [],
  notes: props.initialData?.notes || '',
  url: props.initialData?.url || '',
  images: props.initialData?.images || [] as string[]
})

const showPassword = ref(false)
const newTag = ref('')
const showIdentityDropdown = ref(false)
const imageFileInput = ref<HTMLInputElement | null>(null)
const MAX_IMAGES = 3

// Watch for generated password - always sync
watch(() => props.generatedPassword, (val) => {
  if (val) {
    form.value.password = val
  }
})

// Name autocomplete suggestions
const nameSuggestions = computed(() => {
  if (!form.value.name || !props.allNames) return []
  const query = form.value.name.toLowerCase()
  return props.allNames
    .filter(n => n.toLowerCase().includes(query) && n !== form.value.name)
    .slice(0, 8)
})

// Identity autocomplete suggestions
const identitySuggestions = computed(() => {
  if (!form.value.accountIdentity || !props.allIdentities) return []
  const query = form.value.accountIdentity.toLowerCase()
  return props.allIdentities
    .filter(i => i.toLowerCase().includes(query) && i !== form.value.accountIdentity)
    .slice(0, 8)
})

// Grouped presets for dropdown
const groupedPresets = computed(() => {
  const groups: { category: IdentityCategory; label: string; items: string[] }[] = [
    { category: 'email', label: '邮箱', items: [] },
    { category: 'phone', label: '手机号', items: [] },
    { category: 'id', label: 'ID/用户名', items: [] }
  ]
  props.config.identityPresets.forEach(p => {
    const group = groups.find(g => g.category === p.category)
    if (group) group.items.push(p.value)
  })
  return groups.filter(g => g.items.length > 0)
})

const selectName = (name: string) => {
  form.value.name = name
}

const selectIdentity = (identity: string) => {
  form.value.accountIdentity = identity
  showIdentityDropdown.value = false
}

// Tag input handling
const handleTagClose = (tag: string) => {
  form.value.tags = form.value.tags.filter(t => t !== tag)
}

const handleTagInput = () => {
  if (newTag.value && !form.value.tags.includes(newTag.value)) {
    form.value.tags.push(newTag.value)
    newTag.value = ''
  }
}

const selectTag = (tag: string) => {
  if (!form.value.tags.includes(tag)) {
    form.value.tags.push(tag)
  }
}

// Image handling
const fileToBase64 = (file: File): Promise<string> => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = reject
    reader.readAsDataURL(file)
  })
}

const addImage = async (file: File) => {
  if (form.value.images.length >= MAX_IMAGES) {
    ElMessage.warning(t('最多添加') + ` ${MAX_IMAGES} ` + t('张图片'))
    return
  }
  if (!file.type.startsWith('image/')) {
    ElMessage.warning(t('请选择图片文件'))
    return
  }
  try {
    const base64 = await fileToBase64(file)
    form.value.images.push(base64)
  } catch {
    ElMessage.error(t('图片读取失败'))
  }
}

const handleImageSelect = async (e: Event) => {
  const target = e.target as HTMLInputElement
  const files = target.files
  if (!files) return
  for (const file of Array.from(files)) {
    await addImage(file)
  }
  target.value = ''
}

const handlePaste = async (e: ClipboardEvent) => {
  const items = e.clipboardData?.items
  if (!items) return
  for (const item of Array.from(items)) {
    if (item.type.startsWith('image/')) {
      const file = item.getAsFile()
      if (file) {
        e.preventDefault()
        await addImage(file)
      }
    }
  }
}

const removeImage = (index: number) => {
  form.value.images.splice(index, 1)
}

const isValid = computed(() => {
  return form.value.name.trim() && form.value.password.trim()
})

const handleSubmit = () => {
  if (!isValid.value) return
  emit('submit', {
    name: form.value.name.trim(),
    accountIdentity: form.value.accountIdentity.trim(),
    password: form.value.password,
    tags: form.value.tags,
    notes: form.value.notes.trim(),
    url: form.value.url.trim(),
    images: form.value.images
  })
}
</script>

<template>
  <div class="account-form" @paste="handlePaste">
    <el-form label-position="top" @submit.prevent="handleSubmit">
      <el-form-item :label="t('名称')" required>
        <el-autocomplete
          v-model="form.name"
          :fetch-suggestions="(_query: string, cb: (suggestions: { value: string }[]) => void) => {
            const results = nameSuggestions.map(n => ({ value: n }))
            cb(results)
          }"
          :placeholder="t('例如：WeChat, GitHub')"
          style="width: 100%"
          @select="(item: Record<string, any>) => selectName(item.value)"
        />
      </el-form-item>

      <el-form-item :label="t('账号标识')">
        <div class="identity-input-wrapper">
          <el-autocomplete
            v-model="form.accountIdentity"
            :fetch-suggestions="(_query: string, cb: (suggestions: { value: string }[]) => void) => {
              const results = identitySuggestions.map(i => ({ value: i }))
              cb(results)
            }"
            :placeholder="t('选择或输入账号')"
            class="identity-input"
            @select="(item: Record<string, any>) => selectIdentity(item.value)"
          />
          <el-popover
            v-model:visible="showIdentityDropdown"
            placement="bottom-end"
            :width="280"
            trigger="click"
          >
            <template #reference>
              <el-button class="dropdown-trigger" :icon="ArrowDown" />
            </template>
            <div class="preset-dropdown">
              <div v-if="groupedPresets.length === 0" class="empty-hint">
                {{ t('暂无预设账号') }}
              </div>
              <div v-for="group in groupedPresets" :key="group.category" class="preset-group">
                <div class="group-label">{{ t(group.label) }}</div>
                <div class="group-items">
                  <div
                    v-for="item in group.items"
                    :key="item"
                    class="preset-option"
                    @click="selectIdentity(item)"
                  >
                    {{ item }}
                  </div>
                </div>
              </div>
            </div>
          </el-popover>
        </div>
      </el-form-item>

      <el-form-item :label="t('密码')" required>
        <el-input
          v-model="form.password"
          :type="showPassword ? 'text' : 'password'"
          :placeholder="t('输入或使用生成的密码')"
        >
          <template #suffix>
            <el-icon class="password-toggle" @click="showPassword = !showPassword">
              <View v-if="showPassword" />
              <Hide v-else />
            </el-icon>
          </template>
        </el-input>
      </el-form-item>

      <el-form-item :label="t('APP/服务标签')">
        <div class="tags-container">
          <div class="selected-tags">
            <el-tag
              v-for="tag in form.tags"
              :key="tag"
              closable
              @close="handleTagClose(tag)"
            >
              {{ tag }}
            </el-tag>
          </div>
          <div class="tag-input-row">
            <el-input
              v-model="newTag"
              size="small"
              :placeholder="t('输入新标签后按回车')"
              style="width: 180px"
              @keyup.enter="handleTagInput"
            />
            <el-button size="small" :icon="Plus" @click="handleTagInput">
              {{ t('添加') }}
            </el-button>
          </div>
          <div class="preset-tags">
            <el-tag
              v-for="tag in config.serviceTags"
              :key="tag"
              :type="form.tags.includes(tag) ? 'success' : 'info'"
              effect="plain"
              class="preset-tag"
              @click="selectTag(tag)"
            >
              {{ tag }}
            </el-tag>
          </div>
        </div>
      </el-form-item>

      <el-form-item :label="t('对应链接')">
        <el-input
          v-model="form.url"
          :placeholder="t('例如：https://example.com')"
        />
      </el-form-item>

      <el-form-item :label="t('备注')">
        <el-input
          v-model="form.notes"
          type="textarea"
          :rows="3"
          :placeholder="t('可选备注信息')"
        />
      </el-form-item>

      <el-form-item :label="t('附件') + ` (${form.images.length}/${MAX_IMAGES})`">
        <div class="attachments-container">
          <div class="image-list">
            <div
              v-for="(img, idx) in form.images"
              :key="idx"
              class="image-item"
            >
              <el-image
                :src="img"
                fit="cover"
                class="image-preview"
                :preview-src-list="form.images"
                :initial-index="idx"
              />
              <el-button
                class="image-remove"
                type="danger"
                circle
                size="small"
                :icon="Delete"
                @click="removeImage(idx)"
              />
            </div>
            <div
              v-if="form.images.length < MAX_IMAGES"
              class="image-add"
              @click="imageFileInput?.click()"
            >
              <el-icon :size="24"><Picture /></el-icon>
              <span>{{ t('点击或粘贴') }}</span>
            </div>
          </div>
          <input
            ref="imageFileInput"
            type="file"
            accept="image/*"
            multiple
            style="display: none"
            @change="handleImageSelect"
          />
          <div class="attachments-hint">
            {{ t('支持粘贴 Ctrl+V 或点击上传，最多') }} {{ MAX_IMAGES }} {{ t('张') }}
          </div>
        </div>
      </el-form-item>

      <div class="form-actions">
        <el-button @click="emit('cancel')">{{ t('取消') }}</el-button>
        <el-button type="success" :disabled="!isValid" @click="handleSubmit">
          {{ t('账号入库') }}
        </el-button>
      </div>
    </el-form>
  </div>
</template>

<style scoped>
.account-form {
  padding: 8px;
}
.password-toggle {
  cursor: pointer;
}
.identity-input-wrapper {
  display: flex;
  width: 100%;
  gap: 0;
}
.identity-input {
  flex: 1;
}
.identity-input :deep(.el-input__wrapper) {
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
}
.dropdown-trigger {
  border-top-left-radius: 0;
  border-bottom-left-radius: 0;
  border-left: 0;
}
.preset-dropdown {
  max-height: 300px;
  overflow-y: auto;
}
.empty-hint {
  text-align: center;
  color: var(--el-text-color-placeholder);
  padding: 12px;
}
.preset-group {
  margin-bottom: 12px;
}
.preset-group:last-child {
  margin-bottom: 0;
}
.group-label {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  padding: 4px 8px;
  background: var(--el-fill-color-light);
  border-radius: 4px;
  margin-bottom: 6px;
}
.group-items {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.preset-option {
  padding: 8px 12px;
  cursor: pointer;
  border-radius: 4px;
  font-family: 'JetBrains Mono', 'Consolas', monospace;
  font-size: 13px;
  transition: background 0.2s;
}
.preset-option:hover {
  background: var(--el-color-primary-light-9);
  color: var(--el-color-primary);
}
.tags-container {
  width: 100%;
}
.selected-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 8px;
  min-height: 24px;
}
.tag-input-row {
  display: flex;
  gap: 8px;
  margin-bottom: 8px;
}
.preset-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.preset-tag {
  cursor: pointer;
  transition: transform 0.2s;
}
.preset-tag:hover {
  transform: scale(1.05);
}
.attachments-container {
  width: 100%;
}
.image-list {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}
.image-item {
  position: relative;
  width: 80px;
  height: 80px;
}
.image-preview {
  width: 80px;
  height: 80px;
  border-radius: 6px;
  border: 1px solid var(--el-border-color-lighter);
}
.image-remove {
  position: absolute;
  top: -8px;
  right: -8px;
}
.image-add {
  width: 80px;
  height: 80px;
  border: 2px dashed var(--el-border-color);
  border-radius: 6px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  cursor: pointer;
  color: var(--el-text-color-placeholder);
  transition: border-color 0.2s, color 0.2s;
}
.image-add:hover {
  border-color: var(--el-color-primary);
  color: var(--el-color-primary);
}
.image-add span {
  font-size: 11px;
}
.attachments-hint {
  margin-top: 8px;
  font-size: 12px;
  color: var(--el-text-color-placeholder);
}
.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  margin-top: 24px;
  padding-top: 16px;
  border-top: 1px solid var(--el-border-color-lighter);
}
</style>

<style>
/* JetBrains Mono for account form inputs */
.account-form .el-input__inner[type="password"],
.account-form .identity-input .el-input__inner {
  font-family: 'JetBrains Mono', 'Consolas', monospace !important;
}
</style>
