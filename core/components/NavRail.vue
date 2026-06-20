<script setup lang="ts">
import { ref } from 'vue'
import { Setting } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'
import type { Component } from 'vue'

type Tool = {
  key: string
  name: string
  tag: string
  tagType: 'primary' | 'success' | 'info' | 'warning'
  icon: Component
}

defineProps<{
  tools: Tool[]
  activeToolKey: string
}>()

const emit = defineEmits<{
  (e: 'select', key: string): void
  (e: 'openSettings'): void
  (e: 'collapse-change', collapsed: boolean): void
}>()

const { t } = useSettings()

const collapsed = ref(true)

const onMouseEnter = () => {
  collapsed.value = false
  emit('collapse-change', false)
}

const onMouseLeave = () => {
  collapsed.value = true
  emit('collapse-change', true)
}
</script>

<template>
  <aside
    class="sidebar"
    :class="{ 'is-collapsed': collapsed }"
    @mouseenter="onMouseEnter"
    @mouseleave="onMouseLeave"
  >
    <div class="sidebar-brand">
      <div class="brand-mark">TB</div>
      <div class="brand-text">
        <span class="brand-name">ToolsBag</span>
        <span class="brand-sub">{{ t('精致的桌面工具盒') }}</span>
      </div>
    </div>

    <nav class="sidebar-nav">
      <div class="nav-section-label">{{ t('工具') }}</div>
      <button
        v-for="tool in tools"
        :key="tool.key"
        class="nav-item"
        :class="{ 'is-active': activeToolKey === tool.key }"
        @click="emit('select', tool.key)"
      >
        <el-icon class="nav-item-icon" :size="18"><component :is="tool.icon" /></el-icon>
        <span class="nav-item-label">{{ tool.name }}</span>
        <el-tag
          v-if="tool.tag"
          class="nav-item-tag"
          size="small"
          round
          effect="plain"
          :type="tool.tagType"
        >{{ tool.tag }}</el-tag>
      </button>
    </nav>

    <div class="sidebar-footer">
      <button class="nav-item footer-item" @click="emit('openSettings')">
        <el-icon class="nav-item-icon" :size="18"><Setting /></el-icon>
        <span class="nav-item-label">{{ t('偏好设置') }}</span>
      </button>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  position: fixed;
  top: 0;
  left: 0;
  bottom: 0;
  width: var(--sidebar-width);
  z-index: 42;
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  border-right: 1px solid var(--border-secondary);
  overflow: hidden;
  transition: width var(--duration-normal) var(--ease-out);
}

.sidebar.is-collapsed {
  width: var(--sidebar-collapsed-width);
}

.sidebar-brand {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 18px 16px 14px;
  flex-shrink: 0;
}

.sidebar.is-collapsed .sidebar-brand {
  justify-content: center;
  padding: 18px 0 14px;
}

.brand-mark {
  width: 36px;
  height: 36px;
  border-radius: var(--radius-md);
  display: grid;
  place-items: center;
  background: var(--accent-gradient);
  color: #fff;
  font-weight: 800;
  font-size: 13px;
  flex-shrink: 0;
  user-select: none;
}

.brand-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.sidebar.is-collapsed .brand-text {
  display: none;
}

.brand-name {
  font-size: 15px;
  font-weight: 700;
  color: var(--text-primary);
  line-height: 1.2;
}

.brand-sub {
  font-size: 11px;
  color: var(--text-muted);
  line-height: 1.3;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sidebar-nav {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 4px 10px;
}

.sidebar-nav::-webkit-scrollbar {
  width: 0;
}

.nav-section-label {
  font-size: 10px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  padding: 8px 8px 6px;
  user-select: none;
}

.sidebar.is-collapsed .nav-section-label {
  display: none;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  height: 38px;
  padding: 0 10px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  cursor: pointer;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  transition: all var(--duration-fast) var(--ease-out);
  text-align: left;
}

.sidebar.is-collapsed .nav-item {
  justify-content: center;
  padding: 0;
}

.nav-item:hover {
  background: var(--accent-light);
  color: var(--accent);
}

.nav-item.is-active {
  background: var(--accent-light);
  color: var(--accent);
  font-weight: 600;
  box-shadow: inset 3px 0 0 var(--accent);
}

.nav-item-icon {
  flex-shrink: 0;
}

.nav-item-label {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sidebar.is-collapsed .nav-item-label {
  display: none;
}

.nav-item-tag {
  flex-shrink: 0;
  margin-left: auto;
}

.sidebar.is-collapsed .nav-item-tag {
  display: none;
}

.sidebar-footer {
  flex-shrink: 0;
  padding: 8px 10px 12px;
  border-top: 1px solid var(--border-secondary);
}

.footer-item {
  color: var(--text-muted);
}

.footer-item:hover {
  color: var(--text-primary);
}
</style>
