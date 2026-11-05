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
          {{ t('精致克制的桌面工具盒 · 媒体探针 · 图像套件 · 文件收割 · 密码册') }}
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
  padding: 4px 4px 24px;
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
  font-size: 28px;
  line-height: 34px;
  font-weight: 700;
  letter-spacing: -0.03em;
  color: var(--text-primary);
}
.tool-home__subtitle {
  margin: 6px 0 0;
  font-size: 15px;
  line-height: 20px;
  color: var(--text-secondary);
}

/* ============ Section titles ============ */
.tool-home__section-title {
  margin: 0 0 var(--space-3);
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  letter-spacing: 0;
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
  height: 32px;
  padding: 0 14px;
  border: none;
  border-radius: var(--radius-pill);
  background: var(--surface-block);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: color var(--duration-fast) var(--ease-standard),
              background var(--duration-fast) var(--ease-standard);
}
.recent-chip:hover {
  color: var(--text-primary);
  background: var(--surface-inset);
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
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: var(--space-4);
}

/* ============ Tool card ============ */
.tool-card {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: 18px;
  text-align: left;
  background: var(--home-card-bg);
  border: none;
  border-radius: 20px;
  box-shadow: var(--card-shadow);
  cursor: pointer;
  transition: transform 0.2s var(--ease-out),
              box-shadow var(--duration-normal) var(--ease-standard);
}
.tool-card:hover {
  transform: translateY(-2px);
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
  width: 40px;
  height: 40px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  color: #fff;
  flex-shrink: 0;
  box-shadow: none;
}
.tool-card__tag {
  display: inline-flex;
  align-items: center;
  height: 20px;
  padding: 0 8px;
  border-radius: var(--radius-pill);
  background: var(--soft-badge-bg);
  color: var(--soft-badge-fg);
  font-size: 11px;
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
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.01em;
}
.tool-card__desc {
  margin: 0;
  font-size: 13px;
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
