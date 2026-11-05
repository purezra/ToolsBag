<script setup lang="ts">
import { computed } from 'vue'
import { HomeFilled, Setting } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'
import { useTools } from '@core/hooks/useTools'
import { HOME_KEY } from '@core/tools'

defineProps<{
  /** Expanded width state — rail width transitions via CSS */
  expanded: boolean
}>()

const emit = defineEmits<{
  (e: 'open-settings'): void
}>()

const { t } = useSettings()
const { localizedTools, activeKey, goHome, selectTool } = useTools()

const items = computed(() =>
  localizedTools.value.map((tool) => ({
    key: tool.key,
    name: tool.name,
    tag: tool.tag,
    icon: tool.icon,
    accent: tool.accent
  }))
)

const isHome = computed(() => activeKey.value === HOME_KEY)

const handleItemClick = (key: string) => {
  selectTool(key)
}

const handleHomeClick = () => {
  goHome()
}
</script>

<template>
  <aside class="nav-rail" :class="{ 'is-expanded': expanded }" :aria-label="t('工具导航')">
    <button type="button" class="nav-rail__brand" :title="t('工具中心')" @click="handleHomeClick">
      <span class="brand-mark">TB</span>
      <span class="brand-name">ToolsBag</span>
    </button>

    <div class="nav-rail__top">
      <button
        type="button"
        class="nav-item nav-item--home"
        :class="{ 'is-active': isHome }"
        :aria-current="isHome ? 'page' : undefined"
        :title="t('工具中心')"
        @click="handleHomeClick"
      >
        <el-icon class="nav-item__icon" :size="18"><HomeFilled /></el-icon>
        <span class="nav-item__label">{{ t('工具中心') }}</span>
      </button>
    </div>

    <nav class="nav-rail__list" aria-label="tools">
      <button
        v-for="item in items"
        :key="item.key"
        type="button"
        class="nav-item"
        :class="{ 'is-active': activeKey === item.key }"
        :aria-current="activeKey === item.key ? 'page' : undefined"
        :title="item.name"
        @click="handleItemClick(item.key)"
      >
        <el-icon class="nav-item__icon" :size="18"><component :is="item.icon" /></el-icon>
        <span class="nav-item__label">{{ item.name }}</span>
        <span v-if="item.tag" class="nav-item__tag">{{ item.tag }}</span>
      </button>
    </nav>

    <div class="nav-rail__bottom">
      <button
        type="button"
        class="nav-item nav-item--settings"
        :title="t('偏好设置')"
        @click="emit('open-settings')"
      >
        <el-icon class="nav-item__icon" :size="18"><Setting /></el-icon>
        <span class="nav-item__label">{{ t('偏好设置') }}</span>
      </button>
    </div>
  </aside>
</template>

<style scoped>
.nav-rail {
  width: var(--sidebar-collapsed-width);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 8px 12px;
  background: var(--nav-rail-bg);
  border-right: none;
  transition: width 0.22s var(--ease-out);
  overflow: hidden;
}
.nav-rail.is-expanded {
  width: var(--sidebar-width);
}

/* ============ Brand ============ */
.nav-rail__brand {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 48px;
  padding: 0 8px;
  margin-bottom: 6px;
  border: none;
  background: transparent;
  border-radius: var(--radius-md);
  cursor: pointer;
  overflow: hidden;
}
.nav-rail__brand:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.brand-mark {
  flex-shrink: 0;
  width: 32px;
  height: 32px;
  border-radius: 10px;
  display: grid;
  place-items: center;
  background: var(--accent);
  color: #fff;
  font-weight: 700;
  font-size: 12px;
  letter-spacing: 0.02em;
  user-select: none;
}
.brand-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.02em;
  white-space: nowrap;
  opacity: 0;
  transition: opacity 0.12s ease;
}
.nav-rail.is-expanded .brand-name {
  opacity: 1;
}
.nav-rail:not(.is-expanded) .nav-rail__brand {
  justify-content: center;
  padding: 0;
}
/* Collapsed rail: the hidden name must not reserve layout width, or the
   centered mark overflows the 68px rail and gets clipped. */
.nav-rail:not(.is-expanded) .brand-name {
  display: none;
}

.nav-rail__top {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-bottom: 4px;
}

.nav-rail__list {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 2px 0;
  scrollbar-width: thin;
}
.nav-rail__list::-webkit-scrollbar {
  width: 0;
}

.nav-rail__bottom {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-top: 6px;
}

/* ============ Nav item ============ */
.nav-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  height: 42px;
  padding: 0 12px;
  width: 100%;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  border-radius: var(--radius-md);
  cursor: pointer;
  white-space: nowrap;
  text-align: left;
  transition: background var(--duration-fast) var(--ease-standard),
              color var(--duration-fast) var(--ease-standard);
}
.nav-item:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.nav-item:hover {
  background: var(--nav-item-hover-bg);
  color: var(--text-primary);
}

/* Active: FlClash's filled pill with a primary glyph. */
.nav-item.is-active {
  background: var(--nav-item-active-bg);
  color: var(--nav-item-active-fg);
  font-weight: 600;
}
.nav-item.is-active:hover {
  background: var(--nav-item-active-bg);
}

.nav-item__icon {
  flex-shrink: 0;
  color: inherit;
}
.nav-item.is-active .nav-item__icon {
  color: var(--accent);
}

.nav-rail:not(.is-expanded) .nav-item {
  justify-content: center;
  padding: 0;
}

.nav-item__label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  opacity: 0;
  transition: opacity 0.15s ease 0.05s;
}
.nav-rail.is-expanded .nav-item__label {
  opacity: 1;
}
/* Collapsed rail: remove the label from layout so the icon is truly centered. */
.nav-rail:not(.is-expanded) .nav-item__label {
  display: none;
}

.nav-item__tag {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  height: 18px;
  padding: 0 6px;
  border-radius: var(--radius-pill);
  background: var(--soft-badge-bg);
  color: var(--soft-badge-fg);
  font-size: 10px;
  font-weight: 500;
  opacity: 0;
  transition: opacity 0.15s ease 0.05s;
}
.nav-rail.is-expanded .nav-item__tag {
  opacity: 1;
}
.nav-rail:not(.is-expanded) .nav-item__tag {
  display: none;
}

@media (prefers-reduced-motion: reduce) {
  .nav-rail,
  .nav-item,
  .nav-item__label,
  .nav-item__tag {
    transition: none !important;
  }
}
</style>
