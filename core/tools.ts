/* ============================================================
   ToolsBag — shared tool registry
   Single source of truth for tool metadata + lazy components.
   Consumed by App shell, NavRail, CommandSearch, ToolHome.
   ============================================================ */
import { defineAsyncComponent, type Component } from 'vue'
import { Grid, Picture, FolderChecked, Key } from '@element-plus/icons-vue'

export type ToolTagType = 'primary' | 'success' | 'info' | 'warning'

export interface ToolDefinition {
  key: string
  /** Raw (Chinese) name — localized via t() at call sites */
  name: string
  /** Raw description */
  desc: string
  tag: string
  tagType: ToolTagType
  icon: Component
  iconImage: string
  /** Small icon-tint gradient, used only on tiny icon chips */
  accent: string
  /** Short capability summary shown under the title */
  meta: string
}

/** Lazy-loaded tool page components (code-split per tool) */
export const toolComponentMap: Record<string, Component> = {
  'media-batch': defineAsyncComponent(() => import('@media-batch/index.vue')),
  'image-tools': defineAsyncComponent(() => import('@image-tools/index.vue')),
  'file-traverse': defineAsyncComponent(() => import('@file-traverse/index.vue')),
  'codebook': defineAsyncComponent(() => import('@codebook/index.vue'))
}

export const toolDefinitions: ToolDefinition[] = [
  {
    key: 'media-batch',
    name: '媒体探针',
    desc: '提取视频/图片元数据，批量整理命名，并输出视频体检报告',
    tag: 'V2',
    tagType: 'primary',
    icon: Grid,
    iconImage: '/assets/tool1.webp',
    accent: 'linear-gradient(135deg, #48c6ef, #6f86d6)',
    meta: '元数据提取 · 视频体检 · 批量整理'
  },
  {
    key: 'image-tools',
    name: '图片工坊',
    desc: '集中处理图片批量操作、AVIF/JXL 压缩转换与图片合成 PDF',
    tag: 'Suite',
    tagType: 'success',
    icon: Picture,
    iconImage: '/assets/tool2.webp',
    accent: 'linear-gradient(135deg, #43e97b, #38f9d7)',
    meta: '批量处理 · AVIF/JXL · 图片转PDF'
  },
  {
    key: 'file-traverse',
    name: '文件收割',
    desc: '高速复制、按格式分类与过滤，批量提取整理',
    tag: 'New',
    tagType: 'primary',
    icon: FolderChecked,
    iconImage: '/assets/tool3.webp',
    accent: 'linear-gradient(135deg, #00c6ff, #0072ff)',
    meta: '多线程复制 · 模式过滤'
  },
  {
    key: 'codebook',
    name: '密码册',
    desc: '密码生成器与账号资产管理，安全存储与导出',
    tag: 'New',
    tagType: 'warning',
    icon: Key,
    iconImage: '/assets/tool4.webp',
    accent: 'linear-gradient(135deg, #f093fb, #f5576c)',
    meta: '密码生成 · 账号管理'
  }
]

/** Special key used by the Launcher / Home view */
export const HOME_KEY = '__home__'
