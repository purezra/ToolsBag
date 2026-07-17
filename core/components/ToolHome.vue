<script setup lang="ts">
import { computed } from 'vue'
import { ArrowRight } from '@element-plus/icons-vue'
import { useSettings } from '@core/hooks/useSettings'
import { useTools } from '@core/hooks/useTools'
import type { ToolDefinition } from '@core/tools'

const { t } = useSettings()
const { localizedTools, recentTools, selectTool } = useTools()

/** Split meta string into capability chips */
const toCapabilities = (meta: string): string[] =>
  meta
    .split('·')
    .map((s) => s.trim())
    .filter(Boolean)

const toolsWithCapabilities = computed(() =>
  localizedTools.value.map((tool) => ({
    ...tool,
    capabilities: toCapabilities(tool.meta),
    /** Map key to a soft accent color for the icon dot */
    accentKey: tool.key,
    /** Subtle accent dot color — never a full gradient */
    dotColor: tool.accent
  }))
)

const handleOpen = (tool: ToolDefinition) => {
  selectTool(tool.key)
}

/** Sorted by most recently used, fallback to definition order */
const sortedTools = computed(() => {
  const recent = new Set(recentTools.value.map((t) => t.key))
  return [...toolsWithCapabilities.value].sort((a, b) => {
    const aRecent = recent.has(a.key) ? -1 : 1
    const bRecent = recent.has(b.key) ? -1 : 1
    if (aRecent !== bRecent) return aRecent - bRecent
    return 0
  })
})
</script>

<template>
  <div class="tool-home">
    <!-- Hero intro -->
    <section class="tool-home__hero">
      <div class="tool-home__hero-text">
        <h1 class="tool-home__title">{{ t('ToolsBag 工具中心') }}</h1>
        <p class="tool-home__subtitle">
          {{ t('精致克制的桌面工具盒 · 媒体探针 · 图片工坊 · 文件收割 · 密码册') }}
        </p>
      </div>
      <div class="tool-home__hero-badge">
        <span class="tb-soft-badge tb-soft-badge--accent">v0.2.0</span>
      </div>
    </section>

    <!-- Recent -->
    <section v-if="recentTools.length" class="tool-home__recent">
      <h2 class="tool-home__section-title">{{ t('最近使用') }}</h2>
      <div class="tool-home__recent-row">
        <button
          v-for="tool in recentTools"
          :key="`recent-${tool.key}`"
          type="button"
          class="recent-chip"
          @click="handleOpen(tool)"
        >
          <span class="recent-chip__dot" :style="{ background: tool.accent }"></span>
          <span>{{ tool.name }}</span>
        </button>
      </div>
    </section>

    <!-- All tools -->
    <section class="tool-home__grid-section">
      <h2 class="tool-home__section-title">{{ t('全部工具') }}</h2>
      <div class="tool-home__grid">
        <button
          v-for="tool in sortedTools"
          :key="tool.key"
          type="button"
          class="tool-card"
          @click="handleOpen(tool)"
        >
          <div class="tool-card__top">
            <span class="tool-card__icon" :style="{ background: tool.accent }">
              <el-icon :size="18"><component :is="tool.icon" /></el-icon>
            </span>
            <span v-if="tool.tag" class="tool-card__tag">{{ tool.tag }}</span>
          </div>

          <div class="tool-card__body">
            <h3 class="tool-card__name">{{ tool.name }}</h3>
            <p class="tool-card__desc">{{ tool.desc }}</p>
          </div>

          <div class="tool-card__meta">
            <span
              v-for="cap in tool.capabilities"
              :key="cap"
              class="tb-capability"
            >{{ cap }}</span>
          </div>

          <div class="tool-card__open">
            <span>{{ t('打开') }}</span>
            <el-icon :size="14"><ArrowRight /></el-icon>
          </div>
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.tool-home {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  height: 100%;
  min-height: 0;
  overflow-y: auto;
  padding: var(--space-5) var(--space-6);
}

/* ============ Hero ============ */
.tool-home__hero {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-4);
  flex-shrink: 0;
}
.tool-home__title {
  margin: 0;
  font-size: 22px;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--text-primary);
}
.tool-home__subtitle {
  margin: 6px 0 0;
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.5;
}

/* ============ Section titles ============ */
.tool-home__section-title {
  margin: 0 0 var(--space-3);
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

/* ============ Recent chips ============ */
.tool-home__recent {
  flex-shrink: 0;
}
.tool-home__recent-row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}
.recent-chip {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 28px;
  padding: 0 12px;
  border: 1px solid var(--home-card-border);
  border-radius: var(--radius-pill);
  background: var(--home-card-bg);
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: border-color var(--duration-fast) ease,
              color var(--duration-fast) ease,
              background var(--duration-fast) ease;
}
.recent-chip:hover {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-light);
}
.recent-chip:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
.recent-chip__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

/* ============ Tool grid ============ */
.tool-home__grid-section {
  flex: 1;
  min-height: 0;
}
.tool-home__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: var(--space-3);
}

/* ============ Tool card ============ */
.tool-card {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-4);
  text-align: left;
  background: var(--home-card-bg);
  border: 1px solid var(--home-card-border);
  border-radius: var(--radius-card);
  box-shadow: var(--card-shadow);
  cursor: pointer;
  transition: transform 0.2s var(--ease-out),
              border-color var(--duration-fast) ease,
              box-shadow var(--duration-fast) ease;
}
.tool-card:hover {
  transform: translateY(-2px);
  border-color: var(--accent);
  box-shadow: var(--home-card-hover-shadow);
}
.tool-card:active {
  transform: translateY(0);
}
.tool-card:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.tool-card__top {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
/* Icon: small gradient tile — never a large saturated flood */
.tool-card__icon {
  width: 36px;
  height: 36px;
  border-radius: var(--radius-md);
  display: grid;
  place-items: center;
  color: #fff;
  flex-shrink: 0;
  box-shadow: var(--shadow-xs);
}
.tool-card__tag {
  display: inline-flex;
  align-items: center;
  height: 18px;
  padding: 0 7px;
  border-radius: var(--radius-pill);
  background: var(--soft-badge-bg);
  color: var(--soft-badge-fg);
  font-size: 10px;
  font-weight: 500;
}

.tool-card__body {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-height: 0;
}
.tool-card__name {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.01em;
}
.tool-card__desc {
  margin: 0;
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.tool-card__meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 10px;
  padding-top: 2px;
}

.tool-card__open {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  font-weight: 600;
  color: var(--accent);
  margin-top: auto;
  opacity: 0;
  transform: translateX(-4px);
  transition: opacity 0.2s ease, transform 0.2s var(--ease-out);
}
.tool-card:hover .tool-card__open {
  opacity: 1;
  transform: translateX(0);
}
.tool-card__open .el-icon {
  transition: transform var(--duration-fast) ease;
}
.tool-card:hover .tool-card__open .el-icon {
  transform: translateX(2px);
}

@media (max-width: 820px) {
  .tool-home {
    padding: var(--space-4);
  }
  .tool-home__title {
    font-size: 18px;
  }
  .tool-card__open {
    opacity: 1;
    transform: none;
  }
}

@media (prefers-reduced-motion: reduce) {
  .tool-card,
  .recent-chip,
  .tool-card__open,
  .tool-card__open .el-icon {
    transition: none !important;
  }
  .tool-card:hover {
    transform: none;
  }
}
</style>
