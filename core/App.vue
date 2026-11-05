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

const { t } = provideSettings()
const { activeKey, isHome, activeTool, goHome } = useTools(t)

/** Remember the rail state so a collapsed sidebar stays collapsed on restart. */
const NAV_EXPANDED_KEY = 'toolsbag-nav-expanded'
const navExpanded = ref(localStorage.getItem(NAV_EXPANDED_KEY) !== '0')
const settingsOpen = ref(false)
const aboutOpen = ref(false)

const toggleNav = () => {
  navExpanded.value = !navExpanded.value
  localStorage.setItem(NAV_EXPANDED_KEY, navExpanded.value ? '1' : '0')
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

          <!-- Page title: the home hero carries its own title. -->
          <button
            v-if="!isHome"
            type="button"
            class="topbar-page-title"
            @click="handleBrandClick"
          >
            <span class="topbar-page-title__name">{{ activeTool?.name || 'ToolsBag' }}</span>
          </button>
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
  background: var(--surface-page);
  color: var(--text-primary);
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
  padding: 0 20px;
  border-bottom: none;
  background: transparent;
  flex-shrink: 0;
}

.topbar-left {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  /* Keep the centered search bar from shifting between the launcher and tool pages. */
  min-width: 190px;
}

/* Page title: FlClash's large page header, sitting in the toolbar row. */
.topbar-page-title {
  display: flex;
  align-items: center;
  min-width: 0;
  padding: 4px 6px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  cursor: pointer;
  text-align: left;
}
.topbar-page-title:hover {
  background: var(--bg-hover);
}
.topbar-page-title:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
.topbar-page-title__name {
  font-size: 20px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.02em;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
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
  height: 34px;
  padding: 0 11px;
  border: none;
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: color var(--duration-fast) var(--ease-standard),
              background var(--duration-fast) var(--ease-standard);
}
.topbar-icon-btn:hover {
  color: var(--text-primary);
  background: var(--bg-hover);
}
.topbar-icon-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
.topbar-icon-btn.icon-only {
  padding: 0;
  width: 34px;
  justify-content: center;
}

/* ============ Content ============ */
.app-content {
  flex: 1;
  min-height: 0;
  padding: 0 20px 20px;
  overflow: hidden;
  background: transparent;
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
    height: 56px;
    min-height: 56px;
    padding: 0 12px;
    gap: var(--space-2);
  }
  .topbar-page-title__name {
    font-size: 17px;
  }
  .topbar-icon-btn:not(.icon-only) span {
    display: none;
  }
  .app-content {
    padding: 0 12px 12px;
  }
  .topbar-left {
    min-width: 0;
  }
}
</style>
