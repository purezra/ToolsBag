<script setup lang="ts">
import { useSettings } from '@core/hooks/useSettings'
import type { Component } from 'vue'

type Tool = {
  key: string
  name: string
  desc: string
  tag: string
  tagType: 'primary' | 'success' | 'info' | 'warning'
  icon: Component
}

defineProps<{
  tools: Tool[]
  activeToolKey: string
  isOpen: boolean
}>()

const emit = defineEmits<{
  (e: 'select', key: string): void
  (e: 'mouseleave'): void
}>()

const { t } = useSettings()
</script>

<template>
  <el-aside
    width="260px"
    class="nav-panel"
    :class="{ 'is-open': isOpen }"
    @mouseleave="emit('mouseleave')"
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
      @select="(key: string) => emit('select', key)"
    >
      <el-menu-item v-for="tool in tools" :key="tool.key" :index="tool.key">
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
</template>

<style scoped>
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
  padding: 12px 10px;
  box-sizing: border-box;
  border: none;
  box-shadow: none;
  outline: none;
}

.nav-panel.is-open {
  transform: translateX(0);
  box-shadow: var(--shadow-lg);
  z-index: 43;
}

.brand {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 4px 6px 10px;
}

.brand-mark {
  width: 42px;
  height: 42px;
  border-radius: var(--radius-lg);
  display: grid;
  place-items: center;
  font-weight: 800;
  color: #fff;
  background: var(--accent-gradient);
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
  padding: 6px;
  border-radius: var(--radius-md);
}

.tool-menu :deep(.el-menu-item) {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 40px;
  border-radius: var(--radius-sm);
  line-height: 40px;
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
</style>
