<script setup lang="ts">
import { ref, computed } from 'vue'
import { ScaleToOriginal, FullScreen } from '@element-plus/icons-vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { useSettings } from '@core/hooks/useSettings'

defineProps<{
  modelValue: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', val: boolean): void
}>()

const { t } = useSettings()

const aboutContent = ref('')
const aboutLoading = ref(false)
const aboutFullscreen = ref(false)

const toggleAboutFullscreen = () => {
  aboutFullscreen.value = !aboutFullscreen.value
}

const loadAboutContent = async () => {
  if (aboutContent.value) return
  aboutLoading.value = true
  try {
    const response = await fetch('/about.md')
    if (!response.ok) throw new Error('Failed to fetch')
    const content = await response.text()
    aboutContent.value = content
  } catch (e) {
    console.error('Failed to load about content:', e)
    aboutContent.value = '加载失败，请检查文件是否存在。'
  } finally {
    aboutLoading.value = false
  }
}

const renderedAbout = computed(() => DOMPurify.sanitize(marked(aboutContent.value) as string))
</script>

<template>
  <el-dialog
    :model-value="modelValue"
    :title="t('关于')"
    :width="aboutFullscreen ? '100%' : '720px'"
    :top="aboutFullscreen ? '0' : '5vh'"
    :fullscreen="aboutFullscreen"
    class="about-dialog"
    @update:model-value="(val: boolean) => emit('update:modelValue', val)"
    @open="loadAboutContent"
  >
    <div class="about-content" v-html="renderedAbout"></div>
    <div class="about-fullscreen-btn" @click="toggleAboutFullscreen">
      <el-icon :size="20">
        <ScaleToOriginal v-if="aboutFullscreen" />
        <FullScreen v-else />
      </el-icon>
    </div>
  </el-dialog>
</template>

<style>
.about-content {
  max-height: 70vh;
  overflow-y: auto;
  line-height: 1.7;
  font-size: 14px;
}
.about-content h1, .about-content h2, .about-content h3 {
  margin-top: 16px;
  margin-bottom: 8px;
  color: var(--text-primary);
}
.about-content h3 {
  font-size: 16px;
  border-bottom: 1px solid var(--border-secondary);
  padding-bottom: 8px;
}
.about-content p {
  margin: 8px 0;
  color: var(--text-secondary);
}
.about-content table {
  width: 100%;
  border-collapse: collapse;
  margin: 12px 0;
  font-size: 13px;
}
.about-content th, .about-content td {
  border: 1px solid var(--border-secondary);
  padding: 8px 12px;
  text-align: left;
}
.about-content th {
  background: var(--bg-tertiary);
  font-weight: 600;
  color: var(--text-primary);
}
.about-content tr:nth-child(even) {
  background: var(--bg-glass);
}
.about-content tr:hover {
  background: var(--accent-light);
}
.about-content strong {
  color: var(--accent);
}
.about-content code {
  background: var(--bg-tertiary);
  padding: 2px 6px;
  border-radius: var(--radius-xs);
  font-size: 12px;
}
.about-dialog .el-dialog__body {
  position: relative;
  padding-bottom: 20px;
  overflow: auto;
}
.about-dialog.is-fullscreen .el-dialog__body {
  height: calc(100vh - 80px);
  max-height: none;
  padding-bottom: 20px;
}
.about-dialog.is-fullscreen .about-content {
  max-height: none;
  height: auto;
}
.about-content img {
  max-width: 100%;
  height: auto;
  border-radius: 8px;
  margin: 12px 0;
}
</style>
