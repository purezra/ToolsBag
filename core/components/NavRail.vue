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
  gap: var(--space-2);
  padding: var(--space-2) var(--space-1);
  background: var(--nav-rail-bg);
  border-right: 1px solid var(--nav-rail-border);
  transition: width 0.25s var(--ease-out);
  overflow: hidden;
}
.nav-rail.is-expanded {
  width: var(--sidebar-width);
}

.nav-rail__top {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-bottom: var(--space-2);
  border-bottom: 1px solid var(--nav-rail-border);
}

.nav-rail__list {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow-y: auto;
  overflow-x: hidden;
  padding: var(--space-2) 0;
  scrollbar-width: thin;
}
.nav-rail__list::-webkit-scrollbar {
  width: 0;
}

.nav-rail__bottom {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-top: var(--space-2);
  border-top: 1px solid var(--nav-rail-border);
}

/* ============ Nav item ============ */
.nav-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  height: 40px;
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
  transition: background var(--duration-fast) ease,
              color var(--duration-fast) ease;
}
.nav-item:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.nav-item:hover {
  background: var(--nav-item-hover-bg);
  color: var(--text-primary);
}

/* Active: soft background + inner left bar, never a saturated flood */
.nav-item.is-active {
  background: var(--nav-item-active-bg);
  color: var(--nav-item-active-fg);
  font-weight: 600;
}
.nav-item.is-active::before {
  content: '';
  position: absolute;
  left: 4px;
  top: 50%;
  transform: translateY(-50%);
  width: 3px;
  height: 16px;
  border-radius: var(--radius-pill);
  background: var(--nav-item-active-bar);
}
.nav-item.is-active:hover {
  background: var(--nav-item-active-bg);
}

.nav-item__icon {
  flex-shrink: 0;
  color: inherit;
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

@media (prefers-reduced-motion: reduce) {
  .nav-rail,
  .nav-item,
  .nav-item__label,
  .nav-item__tag {
    transition: none !important;
  }
}
</style>
