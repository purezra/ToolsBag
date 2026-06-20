<script setup lang="ts">
import { ref, computed, onMounted, defineAsyncComponent, type Component } from 'vue'
import { Grid, FolderChecked, Key, Picture } from '@element-plus/icons-vue'

import { provideSettings } from './hooks/useSettings'
import { getClickEffectInstance } from './hooks/useClickEffect'
import ErrorBoundary from './components/ErrorBoundary.vue'
import NavRail from './components/NavRail.vue'
import SettingsDrawer from './components/SettingsDrawer.vue'
import AboutDialog from './components/AboutDialog.vue'

const MediaBatchTool = defineAsyncComponent(() => import('@media-batch/index.vue'))
const ImageToolsTool = defineAsyncComponent(() => import('@image-tools/index.vue'))
const FileTraverseTool = defineAsyncComponent(() => import('@file-traverse/index.vue'))
const CodebookTool = defineAsyncComponent(() => import('@codebook/index.vue'))

type Tool = {
  key: string
  name: string
  desc: string
  tag: string
  tagType: 'primary' | 'success' | 'info' | 'warning'
  icon: Component
  iconImage: string
  accent: string
  meta: string
}

const tools: Tool[] = [
  {
    key: 'media-batch',
    name: '媒体元数据中心',
    desc: '提取视频/图片元数据，批量整理命名，并输出视频体检报告',
    tag: 'V2',
    tagType: 'primary',
    icon: Grid,
    iconImage: '/assets/tool1.webp',
    accent: 'linear-gradient(135deg, #48c6ef, #6f86d6)',
    meta: '元数据提取 · 视频体检 · 批量整理'
  },
  {
    key: 'image-tools',
    name: '图片工具箱',
    desc: '集中处理图片批量操作、AVIF/JXL 压缩转换与图片合成 PDF',
    tag: 'Suite',
    tagType: 'success',
    icon: Picture,
    iconImage: '/assets/tool2.webp',
    accent: 'linear-gradient(135deg, #43e97b, #38f9d7)',
    meta: '批量处理 · AVIF/JXL · 图片转PDF'
  },
  {
    key: 'file-traverse',
    name: '文件遍历提取',
    desc: '高速复制、按格式分类与过滤，批量提取整理',
    tag: 'New',
    tagType: 'primary',
    icon: FolderChecked,
    iconImage: '/assets/tool3.webp',
    accent: 'linear-gradient(135deg, #00c6ff, #0072ff)',
    meta: '多线程复制 · 模式过滤'
  },
  {
    key: 'codebook',
    name: '码本',
    desc: '密码生成器与账号资产管理，安全存储与导出',
    tag: 'New',
    tagType: 'warning',
    icon: Key,
    iconImage: '/assets/tool4.webp',
    accent: 'linear-gradient(135deg, #f093fb, #f5576c)',
    meta: '密码生成 · 账号管理'
  },
]

const activeToolKey = ref<string>(tools[0]!.key)
const { t, showWatermark } = provideSettings()
const settingsOpen = ref(false)
const aboutOpen = ref(false)
const sidebarCollapsed = ref(true)

const clickEffect = getClickEffectInstance()

onMounted(() => {
  clickEffect.loadSettings()
})

const localizedTools = computed(() =>
  tools.map((tool) => ({
    ...tool,
    name: t(tool.name),
    desc: t(tool.desc),
    meta: t(tool.meta)
  }))
)

const activeTool = computed<Tool>(
  () => localizedTools.value.find((item) => item.key === activeToolKey.value) ?? localizedTools.value[0]!
)

const toolComponentMap: Record<string, Component> = {
  'media-batch': MediaBatchTool,
  'image-tools': ImageToolsTool,
  'file-traverse': FileTraverseTool,
  'codebook': CodebookTool
}

const handleToolSelect = (key: string) => {
  activeToolKey.value = key
}

const handleCollapseChange = (collapsed: boolean) => {
  sidebarCollapsed.value = collapsed
}
</script>

<template>
  <div class="app-shell">
    <NavRail
      :tools="localizedTools"
      :active-tool-key="activeToolKey"
      @select="handleToolSelect"
      @open-settings="settingsOpen = true"
      @collapse-change="handleCollapseChange"
    />

    <div
      class="main-area"
      :class="{ 'sidebar-is-collapsed': sidebarCollapsed }"
    >
      <header class="header">
        <div class="header-left">
          <h1 class="header-title">{{ activeTool.name }}</h1>
          <el-tag size="small" round :type="activeTool.tagType" effect="plain" class="header-tag">{{ activeTool.tag }}</el-tag>
        </div>
        <div class="header-right">
          <span class="header-meta">{{ activeTool.meta }}</span>
          <el-tooltip effect="dark" :content="activeTool.desc" placement="bottom">
            <div class="header-icon-wrap">
              <img :src="activeTool.iconImage" :alt="activeTool.name" class="header-icon-img" />
            </div>
          </el-tooltip>
        </div>
      </header>

      <main class="content">
        <div class="tool-wrapper">
          <img
            v-if="showWatermark"
            :src="activeTool.iconImage"
            :alt="activeTool.name"
            class="tool-watermark"
          />
          <ErrorBoundary
            v-if="toolComponentMap[activeToolKey]"
            :key-name="activeToolKey"
            class="tool-component"
          >
            <Suspense>
              <template #default>
                <KeepAlive>
                  <Transition name="tool-fade" mode="out-in">
                    <component
                      :is="toolComponentMap[activeToolKey]"
                      :key="activeToolKey"
                      class="tool-component"
                    />
                  </Transition>
                </KeepAlive>
              </template>
              <template #fallback>
                <div class="tool-skeleton">
                  <el-skeleton :rows="6" animated />
                </div>
              </template>
            </Suspense>
          </ErrorBoundary>
          <section v-else class="placeholder-card">
            <el-empty :description="t('该工具正在路上，敬请期待')" />
          </section>
        </div>
      </main>
    </div>

    <SettingsDrawer
      v-model="settingsOpen"
      @open-about="aboutOpen = true"
    />

    <AboutDialog v-model="aboutOpen" />
  </div>
</template>

<style scoped>
.app-shell {
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  display: flex;
}

.main-area {
  margin-left: var(--sidebar-width);
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  flex: 1;
  min-width: 0;
  transition: margin-left var(--duration-normal) var(--ease-out);
}

.main-area.sidebar-is-collapsed {
  margin-left: var(--sidebar-collapsed-width);
}

.header {
  height: var(--header-height);
  min-height: var(--header-height);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 20px;
  border-bottom: 1px solid var(--border-secondary);
  background: var(--bg-secondary);
  flex-shrink: 0;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.header-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  color: var(--text-primary);
  white-space: nowrap;
}

.header-tag {
  flex-shrink: 0;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-shrink: 0;
}

.header-meta {
  font-size: 12px;
  color: var(--text-muted);
  white-space: nowrap;
}

.header-icon-wrap {
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
  overflow: hidden;
  cursor: pointer;
  transition: transform var(--duration-fast) var(--ease-out);
}

.header-icon-wrap:hover {
  transform: scale(1.1);
}

.header-icon-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.content {
  flex: 1;
  min-height: 0;
  padding: 10px;
  overflow: hidden;
}

.tool-wrapper {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.tool-component {
  position: relative;
  z-index: 1;
  flex: 1;
  overflow: hidden;
  min-height: 0;
}

.tool-watermark {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: min(260px, 34vw);
  height: min(260px, 34vw);
  object-fit: contain;
  opacity: 0.08;
  pointer-events: none;
  z-index: 0;
  filter: grayscale(20%);
}

.tool-skeleton {
  padding: 24px;
}

.tool-fade-enter-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.tool-fade-leave-active {
  transition: opacity 0.1s ease, transform 0.1s ease;
}
.tool-fade-enter-from {
  opacity: 0;
  transform: translateY(6px);
}
.tool-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

@media (max-width: 820px) {
  .header {
    height: 44px;
    min-height: 44px;
    padding: 0 14px;
  }

  .header-meta {
    display: none;
  }

  .content {
    padding: 6px;
  }
}

@media (max-width: 640px) {
  .main-area {
    margin-left: 0;
    width: 100%;
  }
}
</style>
