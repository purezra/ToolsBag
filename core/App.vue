<script setup lang="ts">
import { ref } from 'vue'
import { Setting, InfoFilled, Fold, Expand } from '@element-plus/icons-vue'

import { provideSettings } from './hooks/useSettings'
import { useTools } from './hooks/useTools'
import { toolComponentMap, HOME_KEY } from './tools'

import ErrorBoundary from './components/ErrorBoundary.vue'
import SettingsDrawer from './components/SettingsDrawer.vue'
import AboutDialog from './components/AboutDialog.vue'
import NavRail from './components/NavRail.vue'
import CommandSearch from './components/CommandSearch.vue'
import ToolHome from './components/ToolHome.vue'

const { t, showWatermark, watermarkMode, watermarkOpacity, watermarkScale } = provideSettings()
const { activeKey, isHome, activeTool, goHome } = useTools(t)

const navExpanded = ref(false)
const settingsOpen = ref(false)
const aboutOpen = ref(false)

const toggleNav = () => {
  navExpanded.value = !navExpanded.value
}

const handleBrandClick = () => {
  goHome()
}
</script>

<template>
  <div
    class="app-shell"
    :class="{
      'nav-expanded': navExpanded,
      'is-home': isHome
    }"
  >
    <NavRail :expanded="navExpanded" @open-settings="settingsOpen = true" />

    <div class="app-main">
      <header class="app-topbar">
        <div class="topbar-left">
          <button
            type="button"
            class="topbar-icon-btn icon-only"
            :title="navExpanded ? t('折叠导航') : t('展开导航')"
            :aria-label="navExpanded ? t('折叠导航') : t('展开导航')"
            @click="toggleNav"
          >
            <el-icon :size="17"><Expand v-if="!navExpanded" /><Fold v-else /></el-icon>
          </button>

          <button type="button" class="topbar-brand" @click="handleBrandClick">
            <div class="brand-mark">TB</div>
            <span class="brand-name">ToolsBag</span>
          </button>
        </div>

        <!-- Page title shown when a tool is active -->
        <div v-if="!isHome && activeTool" class="topbar-page-title">
          <span class="topbar-page-title__name">{{ activeTool.name }}</span>
        </div>

        <div class="topbar-center">
          <CommandSearch />
        </div>

        <div class="topbar-right">
          <el-tooltip effect="dark" :content="t('偏好设置')" placement="bottom">
            <button class="topbar-icon-btn icon-only" type="button" @click="settingsOpen = true">
              <el-icon :size="17"><Setting /></el-icon>
            </button>
          </el-tooltip>
          <el-tooltip effect="dark" :content="t('关于')" placement="bottom">
            <button class="topbar-icon-btn icon-only" type="button" @click="aboutOpen = true">
              <el-icon :size="17"><InfoFilled /></el-icon>
            </button>
          </el-tooltip>
        </div>
      </header>

      <main class="app-content">
        <div class="tool-wrapper">
          <!-- 工具水印：4 种叠加模式（静态/平铺/漂浮/呼吸），透明度与大小可调 -->
          <div
            v-if="showWatermark && !isHome && activeTool?.iconImage"
            class="tool-watermark-layer"
            :class="`wm-mode-${watermarkMode}`"
            :style="{
              '--wm-opacity': watermarkOpacity / 100,
              '--wm-scale': watermarkScale / 100,
              '--wm-tile-url': `url('${activeTool.iconImage}')`
            }"
            aria-hidden="true"
          >
            <img :src="activeTool.iconImage" :alt="activeTool.name" class="tool-watermark-img" draggable="false" />
          </div>

          <!-- Launcher / Home -->
          <Transition name="tool-fade" mode="out-in">
            <ToolHome v-if="isHome" key="home" class="tool-component" />
            <!-- Tool page -->
            <ErrorBoundary
              v-else-if="activeKey !== HOME_KEY && toolComponentMap[activeKey]"
              :key-name="activeKey"
              class="tool-component"
            >
              <Suspense>
                <template #default>
                  <KeepAlive>
                    <component :is="toolComponentMap[activeKey]" :key="activeKey" class="tool-component" />
                  </KeepAlive>
                </template>
                <template #fallback>
                  <div class="tool-skeleton">
                    <el-skeleton :rows="6" animated />
                  </div>
                </template>
              </Suspense>
            </ErrorBoundary>
            <section v-else class="placeholder-card" key="placeholder">
              <el-empty :description="t('该工具正在路上，敬请期待')" />
            </section>
          </Transition>
        </div>
      </main>
    </div>

    <SettingsDrawer v-model="settingsOpen" @open-about="aboutOpen = true" />
    <AboutDialog v-model="aboutOpen" />
  </div>
</template>

<style scoped>
.app-shell {
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  display: flex;
  flex-direction: row;
}

/* ============ Main column (topbar + content) ============ */
.app-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* ============ Topbar ============ */
.app-topbar {
  height: var(--header-height);
  min-height: var(--header-height);
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 0 var(--space-4);
  border-bottom: 1px solid var(--topbar-border);
  background: var(--topbar-bg);
  flex-shrink: 0;
}
.app-shell.nav-expanded .app-topbar {
  backdrop-filter: blur(16px) saturate(140%);
  -webkit-backdrop-filter: blur(16px) saturate(140%);
}

.topbar-left {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
}

.topbar-brand {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 0 4px;
  border: none;
  background: transparent;
  cursor: pointer;
}
.topbar-brand:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
}

.brand-mark {
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm);
  display: grid;
  place-items: center;
  background: var(--accent-gradient);
  color: var(--text-inverse);
  font-weight: 700;
  font-size: 11px;
  letter-spacing: 0.02em;
  user-select: none;
}
.brand-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.01em;
  white-space: nowrap;
}

/* Page title shown between left area and center search */
.topbar-page-title {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  padding: 0 var(--space-2);
  border-right: 1px solid var(--border-secondary);
  margin-right: var(--space-2);
}
.topbar-page-title__name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  white-space: nowrap;
}

.topbar-center {
  flex: 1;
  min-width: 0;
  display: flex;
  justify-content: center;
}
.topbar-center > .command-search {
  width: 100%;
  max-width: var(--searchbar-max-width);
}

.topbar-right {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

/* ============ Topbar icon buttons ============ */
.topbar-icon-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 11px;
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: border-color var(--duration-fast) ease,
              color var(--duration-fast) ease,
              background var(--duration-fast) ease;
}
.topbar-icon-btn:hover {
  border-color: var(--border-primary);
  color: var(--text-primary);
  background: var(--hover-bg);
}
.topbar-icon-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
.topbar-icon-btn.icon-only {
  padding: 0;
  width: 30px;
  justify-content: center;
}

/* ============ Content ============ */
.app-content {
  flex: 1;
  min-height: 0;
  padding: var(--space-4);
  overflow: hidden;
  background: var(--content-bg);
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

.tool-watermark-layer {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 0;
  overflow: hidden;
  opacity: var(--wm-opacity, 0.1);
  /* 默认偏淡，避免压住内容；flat 时仍可被工具组件 z-index:1 盖在下方作为背景 */
}
.tool-watermark-img {
  position: absolute;
  top: 50%;
  left: 50%;
  width: min(220px, 26vw);
  height: min(220px, 26vw);
  object-fit: contain;
  user-select: none;
  transform: translate(-50%, -50%) scale(var(--wm-scale, 1));
  filter: grayscale(30%);
}

/* === 模式 1：静态居中 === */
.wm-mode-static .tool-watermark-img {
  /* 仅静态居中，无动画 */
}

/* === 模式 2：平铺叠加 === */
.wm-mode-tile {
  /* 用整图重复铺满，单图尺寸由 --wm-scale 控制 */
}
.wm-mode-tile .tool-watermark-img {
  display: none;
}
.wm-mode-tile::before {
  content: '';
  position: absolute;
  inset: 0;
  background-image: var(--wm-tile-url);
  /* 单格大小随 scale 变化；spacing 通过 background-size 间接控制 */
  background-size: calc(min(220px, 26vw) * var(--wm-scale, 1)) calc(min(220px, 26vw) * var(--wm-scale, 1));
  background-repeat: repeat;
  background-position: center;
}

/* === 模式 3：漂浮移动 === */
.wm-mode-float .tool-watermark-img {
  animation: wm-float 18s ease-in-out infinite;
}

/* === 模式 4：呼吸缩放 === */
.wm-mode-breath .tool-watermark-img {
  animation: wm-breath 6s ease-in-out infinite;
}

@keyframes wm-float {
  0%   { transform: translate(-60%, -60%) scale(var(--wm-scale, 1)); }
  25%  { transform: translate(-40%, -30%) scale(var(--wm-scale, 1)); }
  50%  { transform: translate(-60%, -40%) scale(var(--wm-scale, 1)); }
  75%  { transform: translate(-40%, -65%) scale(var(--wm-scale, 1)); }
  100% { transform: translate(-60%, -60%) scale(var(--wm-scale, 1)); }
}
@keyframes wm-breath {
  0%, 100% { transform: translate(-50%, -50%) scale(calc(var(--wm-scale, 1) * 0.92)); }
  50%      { transform: translate(-50%, -50%) scale(calc(var(--wm-scale, 1) * 1.08)); }
}

.tool-skeleton {
  padding: var(--space-6);
}

.placeholder-card {
  flex: 1;
  display: grid;
  place-items: center;
  z-index: 1;
}

/* ============ Page transition ============ */
.tool-fade-enter-active {
  transition: opacity 0.2s ease, transform 0.2s var(--ease-out);
}
.tool-fade-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.tool-fade-enter-from {
  opacity: 0;
  transform: translateY(8px);
}
.tool-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

@media (prefers-reduced-motion: reduce) {
  .tool-fade-enter-active,
  .tool-fade-leave-active {
    transition: none !important;
  }
}

@media (max-width: 820px) {
  .app-topbar {
    height: 48px;
    min-height: 48px;
    padding: 0 var(--space-2);
    gap: var(--space-2);
  }
  .brand-name {
    display: none;
  }
  .topbar-page-title {
    display: none;
  }
  .topbar-icon-btn:not(.icon-only) span {
    display: none;
  }
  .app-content {
    padding: var(--space-2);
  }
}
</style>
