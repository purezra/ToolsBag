<script setup lang="ts">
import { computed, ref, watch, onMounted, defineAsyncComponent, type Component } from 'vue'
import { Grid, FolderChecked, Setting, DataAnalysis, Moon, Sunny, InfoFilled, Key, FullScreen, ScaleToOriginal, Document, Operation } from '@element-plus/icons-vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

import { provideSettings } from './hooks/useSettings'
import { getClickEffectInstance, type ClickEffectType } from './hooks/useClickEffect'
import ErrorBoundary from './components/ErrorBoundary.vue'

const MediaBatchTool = defineAsyncComponent(() => import('@media-batch/index.vue'))
const ImageBatchTool = defineAsyncComponent(() => import('@image-batch/index.vue'))
const FileTraverseTool = defineAsyncComponent(() => import('@file-traverse/index.vue'))
const CodebookTool = defineAsyncComponent(() => import('@codebook/index.vue'))
const ImageToPdfTool = defineAsyncComponent(() => import('@image-to-pdf/index.vue'))

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
    key: 'image-pdf-lite',
    name: 'PDF合成（原样）',
    desc: '无损合成：按批或全部图片快速生成PDF，显示问题列表',
    tag: 'Lite',
    tagType: 'success',
    icon: DataAnalysis,
    iconImage: '/assets/tool2.webp',
    accent: 'linear-gradient(135deg, #9be15d, #00e3ae)',
    meta: '原样嵌入 · 批量合成'
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
  {
    key: 'image-to-pdf',
    name: '图片合成PDF',
    desc: '高确定性图片合并PDF，支持边距、多尺寸、预览',
    tag: 'Pro',
    tagType: 'primary',
    icon: Document,
    iconImage: '/assets/tool4.webp',
    accent: 'linear-gradient(135deg, #667eea, #764ba2)',
    meta: '比例适配 · 边距系统 · 预览一致'
  }
]

const activeToolKey = ref<string>(tools[0]!.key)
const { 
  t, appearance, setAppearance, locale, setLocale,
  fontFamily, setFontFamily, customFontName, loadCustomFont, clearCustomFont,
  showWatermark, setShowWatermark
} = provideSettings()
const settingsOpen = ref(false)

// Click effect settings
const clickEffect = getClickEffectInstance()
const clickEffectEnabled = ref(false)
const clickEffectType = ref<ClickEffectType>('explosion')
const clickEffectDuration = ref(400)

const durationMarks = computed(() => ({
  150: t('快'),
  400: t('中'),
  700: t('慢')
}))

const handleClickEffectToggle = (val: boolean) => {
  clickEffectEnabled.value = val
  clickEffect.setEnabled(val)
  clickEffect.saveSettings()
}

const handleClickEffectTypeChange = (val: ClickEffectType) => {
  clickEffectType.value = val
  clickEffect.setEffectType(val)
  clickEffect.saveSettings()
}

const handleClickEffectDurationChange = () => {
  clickEffect.setDuration(clickEffectDuration.value)
  clickEffect.saveSettings()
}

const handleFontUpload = async (file: File) => {
  const success = await loadCustomFont(file)
  if (!success) {
    console.error('Failed to load font')
  }
  return false // Prevent default upload
}

onMounted(() => {
  clickEffect.loadSettings()
  clickEffectEnabled.value = clickEffect.enabled.value
  clickEffectType.value = clickEffect.effectType.value
  clickEffectDuration.value = clickEffect.duration.value
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
  'image-pdf-lite': ImageBatchTool,
  'file-traverse': FileTraverseTool,
  'codebook': CodebookTool,
  'image-to-pdf': ImageToPdfTool
}

const navExpanded = ref(false)
const navPinned = ref(false)

// Slide animation direction
const previousToolIndex = ref(0)
const slideDirection = ref<'slide-left' | 'slide-right'>('slide-left')

const currentToolIndex = computed(() =>
  tools.findIndex(t => t.key === activeToolKey.value)
)

// Watch for tool changes to determine slide direction
watch(activeToolKey, (_newKey, _oldKey) => {
  const newIndex = currentToolIndex.value
  slideDirection.value = newIndex >= previousToolIndex.value ? 'slide-left' : 'slide-right'
  previousToolIndex.value = newIndex
})

const toggleNav = () => {
  navExpanded.value = !navExpanded.value
}

const togglePin = () => {
  navPinned.value = !navPinned.value
  if (navPinned.value) {
    navExpanded.value = true
  }
}

const handleNavMouseLeave = () => {
  if (!navPinned.value) {
    navExpanded.value = false
  }
}

// About dialog
const aboutOpen = ref(false)
const aboutContent = ref('')
const aboutLoading = ref(false)
const aboutFullscreen = ref(false)

const toggleAboutFullscreen = () => {
  aboutFullscreen.value = !aboutFullscreen.value
}

const loadAboutContent = async () => {
  if (aboutContent.value) return
  aboutLoading.value = true
  try {
    const response = await fetch('/about.md')
    if (!response.ok) throw new Error('Failed to fetch')
    const content = await response.text()
    aboutContent.value = content
  } catch (e) {
    console.error('Failed to load about content:', e)
    aboutContent.value = '加载失败，请检查文件是否存在。'
  } finally {
    aboutLoading.value = false
  }
}

const handleAboutOpen = () => {
  aboutOpen.value = true
  loadAboutContent()
}

const renderedAbout = computed(() => DOMPurify.sanitize(marked(aboutContent.value) as string))
</script>

<template>
  <div class="app-shell">
    <el-container class="app-frame">
      <!-- Icon rail (always visible) -->
      <div class="nav-rail" @click="toggleNav">
        <div class="rail-brand" :title="t('展开导航')">
          <span class="rail-brand-text">TB</span>
        </div>
        <div class="rail-items">
          <div
            v-for="tool in localizedTools"
            :key="tool.key"
            class="rail-item"
            :class="{ 'is-active': activeToolKey === tool.key }"
            :title="tool.name"
            @click.stop="activeToolKey = tool.key"
          >
            <el-icon :size="20"><component :is="tool.icon" /></el-icon>
          </div>
        </div>
        <div class="rail-footer">
          <div
            class="rail-item"
            :class="{ 'is-pinned': navPinned }"
            :title="navPinned ? t('取消固定') : t('固定导航')"
            @click.stop="togglePin"
          >
            <el-icon :size="18"><Operation /></el-icon>
          </div>
          <div
            class="rail-item"
            :title="t('偏好设置')"
            @click.stop="settingsOpen = true"
          >
            <el-icon :size="18"><Setting /></el-icon>
          </div>
        </div>
      </div>

      <!-- Expanded panel (slides over) -->
      <el-aside
        width="260px"
        class="nav-panel"
        :class="{ 'is-open': navExpanded }"
        @mouseleave="handleNavMouseLeave"
      >
        <div class="brand">
          <div class="brand-mark">TB</div>
          <div>
            <p class="brand-title">ToolsBag</p>
            <p class="brand-sub">{{ t('精致的桌面工具盒') }}</p>
          </div>
        </div>

        <el-menu
          class="tool-menu"
          :default-active="activeToolKey"
          :ellipsis="false"
          @select="(key: string) => (activeToolKey = key)"
        >
          <el-menu-item v-for="tool in localizedTools" :key="tool.key" :index="tool.key">
            <el-icon class="menu-icon">
              <component :is="tool.icon" />
            </el-icon>
            <span>{{ tool.name }}</span>
            <el-tag size="small" round effect="plain" :type="tool.tagType">{{ tool.tag }}</el-tag>
          </el-menu-item>
        </el-menu>

        <div class="nav-footer">
          <div class="meta">
            <p>HarmonyOS Sans</p>
            <span>{{ t('轻盈而克制的触感') }}</span>
          </div>
        </div>
      </el-aside>

      <el-container class="main-wrap">
        <el-header class="topbar">
          <div class="topbar-left">
            <div class="topbar-breadcrumb">
              <span class="topbar-tool-name">{{ activeTool.name }}</span>
              <span class="topbar-sep">/</span>
              <span class="topbar-tool-meta">{{ activeTool.meta }}</span>
            </div>
            <el-tag size="small" round :type="activeTool.tagType" effect="plain" class="topbar-tag">{{ activeTool.tag }}</el-tag>
          </div>
          <div class="topbar-right">
            <el-tooltip effect="dark" :content="activeTool.desc" placement="bottom">
              <div class="topbar-icon-wrap">
                <img :src="activeTool.iconImage" :alt="activeTool.name" class="topbar-icon-img" />
              </div>
            </el-tooltip>
          </div>
        </el-header>

        <el-main class="main-panel">
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
              <Transition :name="slideDirection" mode="out-in">
                <Suspense>
                  <template #default>
                    <KeepAlive>
                      <component
                        :is="toolComponentMap[activeToolKey]"
                        :key="activeToolKey"
                        class="tool-component"
                      />
                    </KeepAlive>
                  </template>
                  <template #fallback>
                    <div class="tool-skeleton">
                      <el-skeleton :rows="6" animated />
                    </div>
                  </template>
                </Suspense>
              </Transition>
            </ErrorBoundary>
            <section v-else class="placeholder-card">
              <el-empty :description="t('该工具正在路上，敬请期待')" />
            </section>
          </div>
        </el-main>
      </el-container>
    </el-container>

    <el-drawer v-model="settingsOpen" size="320px" :title="t('偏好设置')" direction="ltr">
      <div class="setting-group">
        <p class="setting-label">{{ t('明暗') }}</p>
        <el-radio-group
          :model-value="appearance"
          size="small"
          @change="(val: any) => setAppearance(val)"
        >
          <el-radio-button label="light">
            <el-icon><Sunny /></el-icon>
            {{ t('浅色模式') }}
          </el-radio-button>
          <el-radio-button label="dark">
            <el-icon><Moon /></el-icon>
            {{ t('深色模式') }}
          </el-radio-button>
          <el-radio-button label="system">{{ t('跟随系统') }}</el-radio-button>
        </el-radio-group>
      </div>

      <div class="setting-group">
        <p class="setting-label">{{ t('语言') }}</p>
        <el-radio-group :model-value="locale" size="small" @change="(val: any) => setLocale(val)">
          <el-radio-button label="zh">{{ t('中文') }}</el-radio-button>
          <el-radio-button label="en">{{ t('英文') }}</el-radio-button>
        </el-radio-group>
      </div>

      <div class="setting-group">
        <p class="setting-label">{{ t('界面字体') }}</p>
        <el-radio-group :model-value="fontFamily" size="small" @change="(val: any) => setFontFamily(val)">
          <el-radio-button label="harmonyos">{{ t('默认字体') }}</el-radio-button>
          <el-radio-button label="custom" :disabled="!customFontName">
            {{ customFontName || t('自定义') }}
          </el-radio-button>
        </el-radio-group>
        <div class="font-upload-row">
          <el-upload
            :show-file-list="false"
            accept=".ttf,.otf,.woff,.woff2"
            :before-upload="handleFontUpload"
          >
            <el-button size="small" type="primary" plain>{{ t('导入字体') }}</el-button>
          </el-upload>
          <el-button 
            v-if="customFontName" 
            size="small" 
            type="danger" 
            plain 
            @click="clearCustomFont"
          >
            {{ t('清除') }}
          </el-button>
        </div>
        <small class="setting-hint">{{ t('支持 TTF/OTF/WOFF 格式') }}</small>
      </div>

      <div class="setting-group">
        <p class="setting-label">{{ t('工具水印') }}</p>
        <el-switch
          :model-value="showWatermark"
          @change="(val: any) => setShowWatermark(val)"
        />
      </div>

      <div class="setting-group">
        <p class="setting-label">{{ t('鼠标点击动画') }}</p>
        <el-switch
          :model-value="clickEffectEnabled"
          @change="(val: any) => handleClickEffectToggle(val)"
        />
        <template v-if="clickEffectEnabled">
          <div class="effect-options">
            <el-radio-group 
              :model-value="clickEffectType" 
              size="small"
              @change="(val: any) => handleClickEffectTypeChange(val)"
            >
              <el-radio-button label="explosion">{{ t('粒子爆炸') }}</el-radio-button>
              <el-radio-button label="ripple">{{ t('水波纹') }}</el-radio-button>
              <el-radio-button label="halo">{{ t('光晕') }}</el-radio-button>
            </el-radio-group>
            <div class="duration-slider">
              <span class="duration-label">{{ t('消散时间') }} (150-700ms)</span>
              <el-slider
                v-model="clickEffectDuration"
                :min="150"
                :max="700"
                :step="50"
                :marks="durationMarks"
                :show-tooltip="true"
                :format-tooltip="(val: number) => val + 'ms'"
                @change="handleClickEffectDurationChange"
              />
            </div>
          </div>
        </template>
      </div>

      <div class="setting-group">
        <el-button :icon="InfoFilled" @click="handleAboutOpen">{{ t('关于') }}</el-button>
      </div>
    </el-drawer>

    <el-dialog 
      v-model="aboutOpen" 
      :title="t('关于')" 
      :width="aboutFullscreen ? '100%' : '720px'" 
      :top="aboutFullscreen ? '0' : '5vh'"
      :fullscreen="aboutFullscreen"
      class="about-dialog"
    >
      <div class="about-content" v-html="renderedAbout"></div>
      <div class="about-fullscreen-btn" @click="toggleAboutFullscreen">
        <el-icon :size="20">
          <ScaleToOriginal v-if="aboutFullscreen" />
          <FullScreen v-else />
        </el-icon>
      </div>
    </el-dialog>
  </div>
</template>

<style scoped>
.app-shell {
  width: 100vw;
  height: 100vh;
  overflow: hidden;
}

.app-frame {
  width: 100%;
  height: 100%;
}

/* Nav rail - fixed icon bar */
.nav-rail {
  position: fixed;
  top: 0;
  left: 0;
  bottom: 0;
  width: 56px;
  z-index: 42;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 12px 0;
  gap: 8px;
  background: var(--bg-primary);
  border-right: 1px solid var(--border-secondary);
}

.rail-brand {
  width: 40px;
  height: 40px;
  border-radius: var(--radius-md);
  display: grid;
  place-items: center;
  background: var(--accent-gradient);
  color: var(--text-inverse);
  font-weight: 800;
  font-size: 14px;
  cursor: pointer;
  transition: transform var(--duration-fast) var(--ease-out),
              box-shadow var(--duration-fast) var(--ease-out);
  flex-shrink: 0;
}

.rail-brand:hover {
  transform: scale(1.05);
  box-shadow: var(--shadow-sm);
}

.rail-brand-text {
  user-select: none;
}

.rail-items {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 4px 0;
}

.rail-items::-webkit-scrollbar {
  width: 0;
}

.rail-item {
  width: 40px;
  height: 40px;
  border-radius: var(--radius-sm);
  display: grid;
  place-items: center;
  cursor: pointer;
  color: var(--text-muted);
  transition: all var(--duration-fast) var(--ease-out);
  flex-shrink: 0;
}

.rail-item:hover {
  background: var(--accent-light);
  color: var(--accent);
}

.rail-item.is-active {
  background: var(--accent-light);
  color: var(--accent);
  box-shadow: inset 3px 0 0 var(--accent);
}

.rail-item.is-pinned {
  background: var(--accent);
  color: var(--text-inverse);
}

.rail-footer {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
  padding-top: 8px;
  border-top: 1px solid var(--border-secondary);
}

/* Nav panel - slides over rail */
.nav-panel {
  position: fixed;
  top: 0;
  left: 56px;
  bottom: 0;
  width: 260px !important;
  transform: translateX(-260px);
  transition: transform var(--duration-normal) var(--ease-out),
              box-shadow var(--duration-normal) var(--ease-out);
  z-index: 41;
  display: flex;
  flex-direction: column;
  padding: 16px 12px;
  box-sizing: border-box;
}

.nav-panel.is-open {
  transform: translateX(0);
  box-shadow: var(--shadow-lg);
}

.nav-panel.is-open + .main-wrap {
  margin-left: 316px;
  width: calc(100% - 316px);
}

.main-wrap {
  margin-left: 56px;
  width: calc(100% - 56px);
  height: 100%;
  transition: margin-left var(--duration-normal) var(--ease-out),
              width var(--duration-normal) var(--ease-out);
}

.brand {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 6px 12px;
}

.brand-mark {
  width: 42px;
  height: 42px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  font-weight: 800;
  color: #fff;
}

.brand-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
}

.brand-sub {
  margin: 2px 0 0;
  font-size: 12px;
}

.tool-menu {
  flex: 1;
  overflow: auto;
  border-right: 0;
  padding: 8px;
  border-radius: 12px;
}

.tool-menu :deep(.el-menu-item) {
  display: flex;
  align-items: center;
  gap: 8px;
  border-radius: 10px;
}

.menu-icon {
  margin-right: 2px;
}

.nav-footer {
  margin-top: 12px;
  padding: 10px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.meta p {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.meta span {
  font-size: 12px;
  opacity: 0.8;
}

.topbar {
  height: 52px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 20px;
  border-bottom: 1px solid var(--border-secondary);
  background: var(--bg-secondary);
}

.topbar-left {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.topbar-breadcrumb {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.topbar-tool-name {
  font-size: 15px;
  font-weight: 700;
  color: var(--text-primary);
  white-space: nowrap;
}

.topbar-sep {
  font-size: 14px;
  color: var(--text-muted);
  flex-shrink: 0;
}

.topbar-tool-meta {
  font-size: 13px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.topbar-tag {
  flex-shrink: 0;
}

.topbar-right {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}

.topbar-icon-wrap {
  width: 32px;
  height: 32px;
  border-radius: var(--radius-sm);
  overflow: hidden;
  cursor: pointer;
  transition: transform var(--duration-fast) var(--ease-out);
}

.topbar-icon-wrap:hover {
  transform: scale(1.1);
}

.topbar-icon-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.main-panel {
  height: calc(100% - 52px);
  padding: 14px;
  overflow: auto;
}

.setting-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 14px;
}
.setting-label {
  margin: 0;
  font-weight: 600;
  color: #4a536a;
}
.setting-hint {
  color: #7a8194;
}
.font-upload-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.effect-options {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 8px;
  padding: 12px;
  background: #f5f7fa;
  border-radius: 8px;
}
.duration-slider {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.duration-label {
  font-size: 13px;
  color: #606266;
}
.topbar-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
}
.tool-icon-img {
  width: 64px;
  height: 64px;
  object-fit: contain;
  border-radius: 8px;
  opacity: 0.9;
  transition: transform 0.2s, opacity 0.2s;
}
.tool-icon-img:hover {
  transform: scale(1.08);
  opacity: 1;
}
.tool-wrapper {
  position: relative;
  width: 100%;
  height: 100%;
}
.tool-watermark {
  position: fixed;
  top: 50%;
  left: 55%;
  transform: translate(-50%, -50%);
  width: 280px;
  height: 280px;
  object-fit: contain;
  opacity: 0.35;
  pointer-events: none;
  z-index: 9999;
  filter: grayscale(20%);
}
.about-fullscreen-btn {
  position: fixed;
  right: 24px;
  bottom: 24px;
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(64, 158, 255, 0.9);
  border: none;
  border-radius: 10px;
  cursor: pointer;
  color: #fff;
  transition: background 0.2s, transform 0.2s;
  box-shadow: 0 4px 12px rgba(64, 158, 255, 0.4);
  z-index: 10000;
}
.about-fullscreen-btn:hover {
  background: rgba(64, 158, 255, 1);
  transform: scale(1.1);
}
/* Slide left (next tool) */
.slide-left-enter-active,
.slide-left-leave-active {
  transition: opacity 0.25s var(--ease-out), transform 0.25s var(--ease-out);
}
.slide-left-enter-from {
  opacity: 0;
  transform: translateX(30px);
}
.slide-left-leave-to {
  opacity: 0;
  transform: translateX(-30px);
}

/* Slide right (previous tool) */
.slide-right-enter-active,
.slide-right-leave-active {
  transition: opacity 0.25s var(--ease-out), transform 0.25s var(--ease-out);
}
.slide-right-enter-from {
  opacity: 0;
  transform: translateX(-30px);
}
.slide-right-leave-to {
  opacity: 0;
  transform: translateX(30px);
}
.tool-skeleton {
  padding: 24px;
}

/* Responsive */
@media (max-width: 768px) {
  .nav-rail {
    width: 48px;
  }

  .rail-item {
    width: 36px;
    height: 36px;
  }

  .rail-brand {
    width: 36px;
    height: 36px;
    font-size: 12px;
  }

  .nav-panel {
    left: 48px;
    width: 240px !important;
  }

  .main-wrap {
    margin-left: 48px;
    width: calc(100% - 48px);
  }

  .topbar {
    height: 48px;
    padding: 0 12px;
  }

  .topbar-tool-meta {
    display: none;
  }

  .topbar-sep {
    display: none;
  }

  .main-panel {
    height: calc(100% - 48px);
    padding: 8px;
  }

  .topbar-breadcrumb {
    gap: 6px;
  }
}
</style>

<style>
.about-content {
  max-height: 70vh;
  overflow-y: auto;
  line-height: 1.7;
  font-size: 14px;
}
.about-content h1, .about-content h2, .about-content h3 {
  margin-top: 16px;
  margin-bottom: 8px;
  color: #303133;
}
.about-content h3 {
  font-size: 16px;
  border-bottom: 1px solid #ebeef5;
  padding-bottom: 8px;
}
.about-content p {
  margin: 8px 0;
  color: #606266;
}
.about-content table {
  width: 100%;
  border-collapse: collapse;
  margin: 12px 0;
  font-size: 13px;
}
.about-content th, .about-content td {
  border: 1px solid #ebeef5;
  padding: 8px 12px;
  text-align: left;
}
.about-content th {
  background: #f5f7fa;
  font-weight: 600;
  color: #303133;
}
.about-content tr:nth-child(even) {
  background: #fafafa;
}
.about-content tr:hover {
  background: #f0f7ff;
}
.about-content strong {
  color: #409eff;
}
.about-content code {
  background: #f5f7fa;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
}
.about-dialog .el-dialog__body {
  position: relative;
  padding-bottom: 20px;
  overflow: auto;
}
.about-dialog.is-fullscreen .el-dialog__body {
  height: calc(100vh - 80px);
  max-height: none;
  padding-bottom: 20px;
}
.about-dialog.is-fullscreen .about-content {
  max-height: none;
  height: auto;
}
.about-content img {
  max-width: 100%;
  height: auto;
  border-radius: 8px;
  margin: 12px 0;
}
</style>
