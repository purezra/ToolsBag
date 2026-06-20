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
</script>

<template>
  <div class="image-tools-shell">
    <section class="image-tools-tabs">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        class="image-tool-tab"
        :class="{ active: activeTab === tab.key }"
        type="button"
        @click="activeTab = tab.key"
      >
        <el-icon class="tab-icon"><component :is="tab.icon" /></el-icon>
        <span class="tab-text">
          <strong>{{ t(tab.name) }}</strong>
          <small>{{ t(tab.desc) }}</small>
        </span>
      </button>
    </section>

    <section class="image-tools-content">
      <Suspense>
        <template #default>
          <KeepAlive>
            <component :is="activeComponent" :key="activeTab" />
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

<style scoped>
.image-tools-shell {
  display: flex;
  flex-direction: column;
  gap: 10px;
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

.image-tools-tabs {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  flex-shrink: 0;
}

.image-tool-tab {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  padding: 10px 12px;
  border: 1px solid var(--border-secondary);
  border-radius: var(--radius-md);
  background: var(--bg-primary);
  color: var(--text-secondary);
  text-align: left;
  cursor: pointer;
  transition: border-color var(--duration-normal), background var(--duration-normal), color var(--duration-normal), transform var(--duration-fast);
}

.image-tool-tab:hover {
  transform: translateY(-1px);
  border-color: var(--accent);
  color: var(--text-primary);
}

.image-tool-tab.active {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 10%, var(--bg-primary));
  color: var(--text-primary);
}

.tab-icon {
  width: 34px;
  height: 34px;
  border-radius: var(--radius-sm);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-tertiary);
  color: var(--accent);
  font-size: 18px;
  flex-shrink: 0;
}

.tab-text {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.tab-text strong {
  font-size: 13px;
  line-height: 1.2;
}

.tab-text small {
  font-size: 11px;
  line-height: 1.4;
  color: var(--text-muted);
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

@media (max-width: 960px) {
  .image-tools-tabs {
    grid-template-columns: 1fr;
  }

  .tab-text small {
    white-space: normal;
  }
}
</style>
