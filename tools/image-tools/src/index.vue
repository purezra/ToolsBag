<template>
  <div class="image-tools-shell tool-page">
    <div class="image-tools-header">
      <div class="seg-group">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          class="seg-btn"
          :class="{ active: activeTab === tab.key }"
          type="button"
          @click="activeTab = tab.key"
        >
          <el-icon class="seg-icon"><component :is="tab.icon" /></el-icon>
          <span>{{ t(tab.name) }}</span>
        </button>
      </div>
      <span class="image-tools-subtitle">{{ t(activeTabDesc) }}</span>
    </div>

    <section class="image-tools-content">
      <Suspense>
        <template #default>
          <KeepAlive>
            <component :is="activeComponent" />
          </KeepAlive>
        </template>
        <template #fallback>
          <div class="image-tool-skeleton">
            <el-skeleton :rows="6" animated />
          </div>
        </template>
      </Suspense>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, defineAsyncComponent, ref, type Component } from 'vue'
import { Brush, Document, Picture } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'

const ImageBatchTool = defineAsyncComponent(() => import('@image-batch/index.vue'))
const ImageCompressTool = defineAsyncComponent(() => import('@image-compress/index.vue'))
const ImageToPdfTool = defineAsyncComponent(() => import('@image-to-pdf/index.vue'))

type ImageToolTab = {
  key: string
  name: string
  desc: string
  icon: Component
  component: Component
}

const { t } = useSettings()

const tabs: ImageToolTab[] = [
  {
    key: 'batch',
    name: '图片批处理',
    desc: '图片原样合成、批量处理与问题列表',
    icon: Brush,
    component: ImageBatchTool,
  },
  {
    key: 'compress',
    name: '图片压缩/格式转换',
    desc: 'JPG/PNG 本地离线输出 AVIF/JXL，支持 JPEG 原始无损封装',
    icon: Picture,
    component: ImageCompressTool,
  },
  {
    key: 'pdf',
    name: '图片转 PDF',
    desc: '高确定性图片合并 PDF，支持边距、多尺寸与预览',
    icon: Document,
    component: ImageToPdfTool,
  },
]

const activeTab = ref(tabs[0]!.key)
const activeComponent = computed(() => tabs.find(tab => tab.key === activeTab.value)?.component ?? tabs[0]!.component)
const activeTabDesc = computed(() => tabs.find(tab => tab.key === activeTab.value)?.desc ?? tabs[0]!.desc)
</script>

<style scoped>
.image-tools-shell {
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

/* Header: segmented sub-tabs + subtitle (sits below global topbar) */
.image-tools-header {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-shrink: 0;
  flex-wrap: wrap;
}

.seg-group {
  display: inline-flex;
  padding: 3px;
  gap: 2px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-secondary);
  border-radius: var(--radius-md);
}

.seg-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition: background var(--duration-fast) ease, color var(--duration-fast) ease;
}
.seg-btn:hover {
  color: var(--text-primary);
}
.seg-btn.active {
  background: var(--bg-primary);
  color: var(--accent);
  font-weight: 600;
  box-shadow: var(--shadow-xs);
}
.seg-icon {
  font-size: 15px;
}

.image-tools-subtitle {
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.image-tools-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.image-tool-skeleton {
  padding: 16px;
  border: 1px solid var(--border-secondary);
  border-radius: var(--radius-md);
  background: var(--bg-primary);
}

@media (max-width: 720px) {
  .image-tools-subtitle {
    display: none;
  }
}
</style>
