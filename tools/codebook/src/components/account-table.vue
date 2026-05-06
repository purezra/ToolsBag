<script setup lang="ts">
import { ref } from 'vue'
import { View, Hide, CopyDocument, Edit, Delete, Link, Picture, RefreshLeft } from '@element-plus/icons-vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import type { Account } from '@codebook/types/codebook'
import { useSettings } from '@core/hooks/useSettings'

const props = defineProps<{
  accounts: Account[]
  mode?: 'active' | 'recycle'
}>()

const emit = defineEmits<{
  edit: [account: Account]
  delete: [id: string]
  restore: [id: string]
}>()

const { t } = useSettings()

const visiblePasswords = ref<Set<string>>(new Set())

const togglePassword = (id: string) => {
  if (visiblePasswords.value.has(id)) {
    visiblePasswords.value.delete(id)
  } else {
    visiblePasswords.value.add(id)
  }
}

const maskPassword = (password: string) => {
  return '*'.repeat(Math.min(password.length, 12))
}

const copyPassword = async (password: string) => {
  try {
    await navigator.clipboard.writeText(password)
    ElMessage.success(t('已复制密码'))
  } catch {
    ElMessage.error(t('复制失败'))
  }
}

const copyText = async (text: string) => {
  try {
    await navigator.clipboard.writeText(text)
    ElMessage.success(t('已复制'))
  } catch {
    ElMessage.error(t('复制失败'))
  }
}

const openUrl = (url: string) => {
  if (url) {
    window.open(url.startsWith('http') ? url : `https://${url}`, '_blank')
  }
}

const confirmDelete = (account: Account) => {
  ElMessageBox.confirm(
    t('删除后将进入回收站，可在任意设备恢复'),
    t('删除确认'),
    {
      confirmButtonText: t('删除'),
      cancelButtonText: t('取消'),
      type: 'warning'
    }
  )
    .then(() => {
      emit('delete', account.id)
    })
    .catch(() => {})
}

const confirmRestore = (account: Account) => {
  ElMessageBox.confirm(
    t('确定要恢复该账号吗？'),
    t('恢复确认'),
    {
      confirmButtonText: t('恢复'),
      cancelButtonText: t('取消'),
      type: 'primary'
    }
  )
    .then(() => {
      emit('restore', account.id)
    })
    .catch(() => {})
}

const formatDate = (dateStr: string) => {
  try {
    return new Date(dateStr).toLocaleDateString()
  } catch {
    return dateStr
  }
}

const getTagType = (index: number) => {
  const types = [undefined, 'success', 'warning', 'danger', 'info'] as const
  return types[index % types.length]
}
</script>

<template>
  <div class="account-table">
    <el-table
      :data="accounts"
      stripe
      :style="{ width: '100%' }"
      :empty-text="t('暂无账号数据')"
      row-key="id"
    >
      <el-table-column prop="name" :label="t('名称')" width="auto" min-width="100" sortable>
        <template #default="{ row }">
          <span class="name-cell">{{ row.name }}</span>
        </template>
      </el-table-column>

      <el-table-column prop="accountIdentity" :label="t('账号')" width="auto" min-width="140" show-overflow-tooltip>
        <template #default="{ row }">
          <div class="account-cell" v-if="row.accountIdentity">
            <span class="mono-text">{{ row.accountIdentity }}</span>
            <el-icon class="copy-icon" @click.stop="copyText(row.accountIdentity)">
              <CopyDocument />
            </el-icon>
          </div>
          <span v-else class="empty-text">-</span>
        </template>
      </el-table-column>

      <el-table-column :label="t('密码')" width="auto" min-width="160">
        <template #default="{ row }">
          <div class="password-cell">
            <span class="password-text mono-text">{{ visiblePasswords.has(row.id) ? row.password : maskPassword(row.password) }}</span>
            <el-icon class="pwd-action" @click.stop="togglePassword(row.id)">
              <View v-if="visiblePasswords.has(row.id)" />
              <Hide v-else />
            </el-icon>
            <el-icon class="pwd-action" @click.stop="copyPassword(row.password)">
              <CopyDocument />
            </el-icon>
          </div>
        </template>
      </el-table-column>

      <el-table-column :label="t('APP/服务')" width="auto" min-width="120">
        <template #default="{ row }">
          <div class="tags-cell" v-if="row.tags?.length">
            <el-tag
              v-for="(tag, idx) in row.tags.slice(0, 2)"
              :key="tag"
              size="small"
              :type="getTagType(idx)"
            >
              {{ tag }}
            </el-tag>
            <el-tag v-if="row.tags.length > 2" size="small" type="info">
              +{{ row.tags.length - 2 }}
            </el-tag>
          </div>
          <span v-else class="empty-text">-</span>
        </template>
      </el-table-column>

      <el-table-column prop="notes" :label="t('备注')" width="auto" min-width="80" show-overflow-tooltip>
        <template #default="{ row }">
          <span v-if="row.notes">{{ row.notes }}</span>
          <span v-else class="empty-text">-</span>
        </template>
      </el-table-column>

      <el-table-column :label="t('链接')" width="70" align="center">
        <template #default="{ row }">
          <el-button
            v-if="row.url"
            link
            type="primary"
            :icon="Link"
            @click.stop="openUrl(row.url)"
          />
          <span v-else class="empty-text">-</span>
        </template>
      </el-table-column>

      <el-table-column :label="t('图片')" width="70" align="center">
        <template #default="{ row }">
          <el-popover
            v-if="row.images?.length"
            placement="left"
            :width="280"
            trigger="hover"
          >
            <template #reference>
              <div class="image-indicator">
                <el-icon><Picture /></el-icon>
                <span class="mono-text">{{ row.images.length }}</span>
              </div>
            </template>
            <div class="image-preview-list">
              <el-image
                v-for="(img, idx) in row.images"
                :key="idx"
                :src="img"
                fit="cover"
                class="preview-thumb"
                :preview-src-list="row.images"
                :initial-index="idx"
              />
            </div>
          </el-popover>
          <span v-else class="empty-text">-</span>
        </template>
      </el-table-column>

      <el-table-column prop="createdAt" :label="t('创建时间')" width="100" sortable>
        <template #default="{ row }">
          <span class="date-text mono-text">{{ formatDate(row.createdAt) }}</span>
        </template>
      </el-table-column>

      <el-table-column :label="t('操作')" width="100" fixed="right">
        <template #default="{ row }">
          <template v-if="props.mode !== 'recycle'">
            <el-button link type="primary" :icon="Edit" @click.stop="emit('edit', row)" />
            <el-button link type="danger" :icon="Delete" @click.stop="confirmDelete(row)" />
          </template>
          <template v-else>
            <el-button link type="primary" :icon="RefreshLeft" @click.stop="confirmRestore(row)" />
          </template>
        </template>
      </el-table-column>
    </el-table>
  </div>
</template>

<style scoped>
.account-table {
  width: 100%;
}
.name-cell {
  font-weight: 500;
}
.account-cell {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.copy-icon {
  cursor: pointer;
  opacity: 0.4;
  transition: opacity 0.2s;
  flex-shrink: 0;
}
.copy-icon:hover {
  opacity: 1;
  color: var(--el-color-primary);
}
.password-cell {
  display: inline-flex;
  align-items: center;
  gap: 0;
}
.password-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pwd-action {
  cursor: pointer;
  opacity: 0.4;
  transition: opacity 0.2s;
  margin-left: 4px;
  flex-shrink: 0;
}
.pwd-action:hover {
  opacity: 1;
  color: var(--el-color-primary);
}
.tags-cell {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.empty-text {
  color: var(--el-text-color-placeholder);
}
.date-text {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
.image-indicator {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
  color: var(--el-color-primary);
}
.image-preview-list {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.preview-thumb {
  width: 80px;
  height: 80px;
  border-radius: 4px;
  cursor: pointer;
}
.mono-text {
  font-family: 'JetBrains Mono', 'Consolas', monospace;
}
</style>
