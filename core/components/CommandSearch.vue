<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { Search } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'
import { useTools } from '@core/hooks/useTools'
import type { ToolDefinition } from '@core/tools'

const { t } = useSettings()
const { localizedTools, recentTools, selectTool } = useTools()

const query = ref('')
const open = ref(false)
const activeIndex = ref(0)
const inputRef = ref<HTMLInputElement | null>(null)

/** Results: when query empty → recent tools (or all if none), else filtered. */
const results = computed<ToolDefinition[]>(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) {
    return recentTools.value.length ? recentTools.value : localizedTools.value
  }
  return localizedTools.value.filter((tool) => {
    const haystack = `${tool.name} ${tool.desc} ${tool.meta} ${tool.key} ${tool.tag}`.toLowerCase()
    return haystack.includes(q)
  })
})

const hasResults = computed(() => results.value.length > 0)

const clampIndex = () => {
  if (activeIndex.value >= results.value.length) activeIndex.value = 0
}

const openPanel = () => {
  open.value = true
  activeIndex.value = 0
}

const closePanel = () => {
  open.value = false
}

const pick = (tool: ToolDefinition | undefined) => {
  if (!tool) return
  selectTool(tool.key)
  query.value = ''
  closePanel()
  inputRef.value?.blur()
}

const onKeydown = (e: KeyboardEvent) => {
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    if (!open.value) openPanel()
    activeIndex.value = Math.min(activeIndex.value + 1, results.value.length - 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    activeIndex.value = Math.max(activeIndex.value - 1, 0)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    pick(results.value[activeIndex.value])
  } else if (e.key === 'Escape') {
    e.preventDefault()
    query.value = ''
    closePanel()
    inputRef.value?.blur()
  }
}

const handleFocus = () => {
  openPanel()
}

const handleBlur = (e: FocusEvent) => {
  const related = e.relatedTarget as HTMLElement | null
  if (related && (related.closest('.command-search'))) return
  closePanel()
}

// Keep active index valid as results change
watch(results, () => {
  clampIndex()
})

onBeforeUnmount(() => {
  closePanel()
})
</script>

<template>
  <div class="command-search">
    <el-icon class="command-search__icon" :size="15"><Search /></el-icon>
    <input
      ref="inputRef"
      v-model="query"
      type="text"
      class="command-search__input"
      :placeholder="t('搜索工具、命令或最近任务…')"
      autocomplete="off"
      spellcheck="false"
      @focus="handleFocus"
      @blur="handleBlur"
      @keydown="onKeydown"
    />
    <kbd v-if="!open" class="command-search__kbd">
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M15 6v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3V6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3" />
      </svg>
    </kbd>

    <Transition name="cmd-pop">
      <div v-if="open" class="command-search__panel" role="listbox" aria-label="tool results">
        <div v-if="!query.trim()" class="command-search__section">
          {{ recentTools.length ? t('最近使用') : t('全部工具') }}
        </div>
        <div v-else-if="hasResults" class="command-search__section">
          {{ t('结果') }} · {{ results.length }}
        </div>

        <button
          v-for="(tool, idx) in results"
          :key="tool.key"
          type="button"
          class="cmd-item"
          :class="{ 'is-active': idx === activeIndex }"
          role="option"
          :aria-selected="idx === activeIndex"
          @mousedown.prevent="pick(tool)"
          @mouseenter="activeIndex = idx"
        >
          <span class="cmd-item__icon" :style="{ background: tool.accent }">
            <el-icon :size="15"><component :is="tool.icon" /></el-icon>
          </span>
          <span class="cmd-item__body">
            <span class="cmd-item__name">{{ tool.name }}</span>
            <span class="cmd-item__desc">{{ tool.desc }}</span>
          </span>
          <span v-if="tool.tag" class="cmd-item__tag">{{ tool.tag }}</span>
        </button>

        <div v-if="!hasResults" class="command-search__empty">
          {{ t('未找到匹配的工具') }}
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.command-search {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  max-width: var(--searchbar-max-width);
  height: 34px;
  padding: 0 12px;
  background: var(--search-bg);
  border: 1px solid var(--search-border);
  border-radius: var(--radius-md);
  transition: border-color var(--duration-fast) ease,
              background var(--duration-fast) ease,
              box-shadow var(--duration-fast) ease;
}
.command-search:focus-within {
  background: var(--search-bg-focus);
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-light);
}

.command-search__icon {
  color: var(--text-muted);
  flex-shrink: 0;
}

.command-search__input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 13px;
  font-family: inherit;
}
.command-search__input::placeholder {
  color: var(--text-muted);
}

.command-search__kbd {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 20px;
  width: 20px;
  border-radius: var(--radius-sm);
  background: var(--soft-badge-bg);
  color: var(--text-muted);
  opacity: 0.7;
}

/* ============ Results popover ============ */
.command-search__panel {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  right: 0;
  z-index: 50;
  max-height: 380px;
  overflow-y: auto;
  padding: 6px;
  background: var(--bg-primary);
  border: 1px solid var(--border-primary);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
}
.command-search__section {
  padding: 6px 10px 4px;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-muted);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.command-search__empty {
  padding: 18px 10px;
  text-align: center;
  font-size: 12px;
  color: var(--text-muted);
}

.cmd-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  background: transparent;
  color: var(--text-primary);
  text-align: left;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background var(--duration-fast) ease;
}
.cmd-item.is-active {
  background: var(--active-bg);
}

.cmd-item__icon {
  flex-shrink: 0;
  width: 26px;
  height: 26px;
  border-radius: var(--radius-sm);
  display: grid;
  place-items: center;
  color: #fff;
}

.cmd-item__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.cmd-item__name {
  font-size: 13px;
  font-weight: 600;
  line-height: 1.2;
}
.cmd-item__desc {
  font-size: 11px;
  color: var(--text-muted);
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cmd-item__tag {
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
}

/* Popover transition */
.cmd-pop-enter-active,
.cmd-pop-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.cmd-pop-enter-from,
.cmd-pop-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

@media (max-width: 820px) {
  .command-search__kbd {
    display: none;
  }
}
@media (prefers-reduced-motion: reduce) {
  .command-search,
  .cmd-item,
  .cmd-pop-enter-active,
  .cmd-pop-leave-active {
    transition: none !important;
  }
}
</style>
