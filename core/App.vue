<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted, type Component } from 'vue'
import { Grid, FolderChecked, Setting, DataAnalysis, Moon, Sunny, Brush, InfoFilled, Key, FullScreen, ScaleToOriginal, Document } from '@element-plus/icons-vue'
import { marked } from 'marked'

import MediaBatchTool from '@media-batch/index.vue'
import ImageBatchTool from '@image-batch/index.vue'
import FileTraverseTool from '@file-traverse/index.vue'
import CodebookTool from '@codebook/index.vue'
import ImageToPdfTool from '@image-to-pdf/index.vue'
import { provideSettings } from './hooks/useSettings'
import { getClickEffectInstance, type ClickEffectType } from './hooks/useClickEffect'
import ErrorBoundary from './components/ErrorBoundary.vue'

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
    name: '媒体批处理',
    desc: '批量提取视频/图片信息，重命名与可视化分析，一站式处理',
    tag: 'V2',
    tagType: 'primary',
    icon: Grid,
    iconImage: '/assets/tool1.webp',
    accent: 'linear-gradient(135deg, #48c6ef, #6f86d6)',
    meta: '并行提取 · 重命名 · 数据分析'
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
    iconImage: '/assets/tool5.png',
    accent: 'linear-gradient(135deg, #667eea, #764ba2)',
    meta: '比例适配 · 边距系统 · 预览一致'
  }
]

const activeToolKey = ref<string>(tools[0]!.key)
const { 
  t, skin, appearance, setSkin, setAppearance, locale, setLocale,
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
  window.addEventListener('mousemove', handleGlobalMouseMove)
})

onUnmounted(() => {
  window.removeEventListener('mousemove', handleGlobalMouseMove)
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
const navPinnedByHover = ref(false)
const NAV_OPEN_THRESHOLD = 56
const NAV_CLOSE_THRESHOLD = 340

const handleGlobalMouseMove = (event: MouseEvent) => {
  if (navPinnedByHover.value) return
  const x = event.clientX
  if (x <= NAV_OPEN_THRESHOLD) {
    navExpanded.value = true
  } else if (x >= NAV_CLOSE_THRESHOLD) {
    navExpanded.value = false
  }
}

const handleNavEnter = () => {
  navPinnedByHover.value = true
  navExpanded.value = true
}

const handleNavLeave = (event: MouseEvent) => {
  navPinnedByHover.value = false
  if (event.clientX >= NAV_CLOSE_THRESHOLD) {
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

const renderedAbout = computed(() => marked(aboutContent.value) as string)
</script>

<template>
  <div class="app-shell">
    <el-container class="app-frame">
      <div class="nav-hit" @mouseenter="navExpanded = true"></div>
      <el-aside
        width="260px"
        class="nav-panel"
        :class="{ 'is-open': navExpanded }"
        @mouseenter="handleNavEnter"
        @mouseleave="handleNavLeave"
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
          <el-tooltip effect="dark" :content="t('偏好设置')">
            <el-button circle type="primary" :icon="Setting" size="small" @click="settingsOpen = true" />
          </el-tooltip>
        </div>
      </el-aside>

      <el-container class="main-wrap">
        <el-header class="topbar">
          <div class="topbar-left">
            <div class="pill">{{ activeTool.tag }}</div>
            <span class="hint">{{ activeTool.name }} · {{ activeTool.meta }}</span>
          </div>
          <div class="topbar-right">
            <img :src="activeTool.iconImage" :alt="activeTool.name" class="tool-icon-img" />
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
              <component
                :is="toolComponentMap[activeToolKey]"
                :key="activeToolKey"
                class="tool-component"
              />
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
        <p class="setting-label">{{ t('界面风格') }}</p>
        <el-radio-group :model-value="skin" size="small" @change="(val: any) => setSkin(val)">
          <el-radio-button label="modern">{{ t('现代风格') }}</el-radio-button>
          <el-radio-button label="apple">
            <el-icon><Brush /></el-icon>
            {{ t('Apple store ui') }}
          </el-radio-button>
          <el-radio-button label="smartisan">
            <el-icon><Brush /></el-icon>
            {{ t('Smartisan 风格') }}
          </el-radio-button>
        </el-radio-group>
      </div>

      <div class="setting-group">
        <p class="setting-label">{{ t('明暗') }}</p>
        <el-switch
          :model-value="appearance"
          :active-value="'dark'"
          :inactive-value="'light'"
          inline-prompt
          :active-icon="Moon"
          :inactive-icon="Sunny"
          :disabled="skin === 'smartisan' || skin === 'apple'"
          @change="(val: any) => setAppearance(val)"
        />
        <small class="setting-hint" v-if="skin === 'smartisan'">
          {{ t('Smartisan 风格默认使用木纹浅色方案') }}
        </small>
        <small class="setting-hint" v-else-if="skin === 'apple'">
          {{ t('Apple store ui') }} {{ t('默认使用浅色方案') }}
        </small>
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

.nav-hit {
  position: fixed;
  top: 0;
  left: 0;
  bottom: 0;
  width: 56px;
  z-index: 40;
}

.nav-panel {
  position: fixed;
  top: 0;
  left: 0;
  bottom: 0;
  width: 260px !important;
  transform: translateX(-228px);
  transition: transform 220ms ease, opacity 220ms ease, box-shadow 220ms ease;
  z-index: 41;
  display: flex;
  flex-direction: column;
  padding: 16px 12px;
  box-sizing: border-box;
}

.nav-panel.is-open {
  transform: translateX(0);
}

.main-wrap {
  margin-left: 32px;
  width: calc(100% - 32px);
  height: 100%;
  transition: margin-left 220ms ease, width 220ms ease;
}

.nav-panel.is-open + .main-wrap {
  margin-left: 260px;
  width: calc(100% - 260px);
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
  height: 76px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 18px;
}

.topbar-left {
  display: flex;
  align-items: center;
  gap: 10px;
}

.pill {
  height: 26px;
  line-height: 26px;
  padding: 0 10px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 700;
}

.hint {
  font-size: 13px;
}

.main-panel {
  height: calc(100% - 76px);
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
  transition: all 0.2s;
  box-shadow: 0 4px 12px rgba(64, 158, 255, 0.4);
  z-index: 10000;
}
.about-fullscreen-btn:hover {
  background: rgba(64, 158, 255, 1);
  transform: scale(1.1);
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
