/* ============================================================
   useTools — active tool selection + recent-tool tracking
   Recent tools persist to localStorage (max 5) so the Launcher
   and command bar can surface them across sessions.
   ============================================================ */
import { computed, ref, watch } from 'vue'
import { toolDefinitions, HOME_KEY, type ToolDefinition } from '../tools'
import { useSettings } from './useSettings'

type TranslateFn = (key: string, vars?: Record<string, string | number>) => string

const RECENT_KEY = 'toolsbag-recent-tools'
const RECENT_MAX = 5

/** Load recent tool keys from localStorage, filtered against known tools. */
const loadRecent = (): string[] => {
  try {
    const raw = localStorage.getItem(RECENT_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    const known = new Set(toolDefinitions.map((t) => t.key))
    return parsed.filter((k): k is string => typeof k === 'string' && known.has(k))
  } catch {
    return []
  }
}

// ponytail: one app-wide selection store; add a state library only if this grows beyond tool navigation.
const activeKey = ref<string>(HOME_KEY)
const recentKeys = ref<string[]>(loadRecent())

const persistRecent = () => {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recentKeys.value))
  } catch {
    /* storage may be unavailable; ignore */
  }
}

const recordRecent = (key: string) => {
  const next = [key, ...recentKeys.value.filter((k) => k !== key)].slice(0, RECENT_MAX)
  if (next.join(',') !== recentKeys.value.join(',')) {
    recentKeys.value = next
    persistRecent()
  }
}

watch(
  recentKeys,
  (keys) => {
    const known = new Set(toolDefinitions.map((t) => t.key))
    if (keys.some((k) => !known.has(k))) {
      recentKeys.value = keys.filter((k) => known.has(k))
    }
  },
  { deep: true }
)

export const useTools = (t?: TranslateFn) => {
  const translate = t ?? useSettings().t

  const localizedTools = computed<ToolDefinition[]>(() =>
    toolDefinitions.map((tool) => ({
      ...tool,
      name: translate(tool.name),
      desc: translate(tool.desc),
      meta: translate(tool.meta)
    }))
  )

  const isHome = computed(() => activeKey.value === HOME_KEY)

  const activeTool = computed<ToolDefinition | null>(() => {
    if (activeKey.value === HOME_KEY) return null
    return localizedTools.value.find((item) => item.key === activeKey.value) ?? null
  })

  const recentTools = computed<ToolDefinition[]>(() => {
    const byKey = new Map(localizedTools.value.map((tool) => [tool.key, tool]))
    return recentKeys.value
      .map((k) => byKey.get(k))
      .filter((tool): tool is ToolDefinition => Boolean(tool))
  })

  const goHome = () => {
    activeKey.value = HOME_KEY
  }

  const selectTool = (key: string) => {
    if (!toolDefinitions.some((tool) => tool.key === key)) return
    activeKey.value = key
    recordRecent(key)
  }

  return {
    tools: toolDefinitions,
    localizedTools,
    activeKey,
    isHome,
    recentTools,
    activeTool,
    goHome,
    selectTool
  }
}
