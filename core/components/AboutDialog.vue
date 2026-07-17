<script setup lang="ts">
import { useSettings } from '@core/hooks/useSettings'

defineProps<{
  modelValue: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', val: boolean): void
}>()

const { t } = useSettings()

const capabilities = [
  {
    index: '01',
    name: '媒体探针',
    description: '查看视频与图片元数据，筛选文件并批量整理命名。'
  },
  {
    index: '02',
    name: '图片工坊',
    description: '批量转换、压缩图片，并完成常用的图片文档处理。'
  },
  {
    index: '03',
    name: '文件收割',
    description: '按类型快速遍历、归集和整理文件。'
  },
  {
    index: '04',
    name: '密码册',
    description: '生成密码，并在本地管理账号资料。'
  }
]

const principles = ['本地优先', '轻量运行', '专注效率']
</script>

<template>
  <el-dialog
    :model-value="modelValue"
    width="min(640px, calc(100vw - 32px))"
    class="about-dialog"
    append-to-body
    @update:model-value="(val: boolean) => emit('update:modelValue', val)"
  >
    <div class="about-hero">
      <div class="about-brand-mark" aria-hidden="true">TB</div>
      <div class="about-heading">
        <span class="about-eyebrow">DESKTOP TOOLKIT</span>
        <h2>ToolsBag</h2>
        <p>{{ t('为日常文件工作提供清晰、可靠的本地工具。') }}</p>
      </div>
      <span class="about-version">v0.2.0</span>
    </div>

    <section class="about-section" aria-labelledby="about-capabilities-title">
      <div class="about-section-head">
        <h3 id="about-capabilities-title">{{ t('核心能力') }}</h3>
        <span>04 TOOLS</span>
      </div>

      <div class="capability-list">
        <div v-for="item in capabilities" :key="item.index" class="capability-row">
          <span class="capability-index">{{ item.index }}</span>
          <strong>{{ t(item.name) }}</strong>
          <p>{{ t(item.description) }}</p>
        </div>
      </div>
    </section>

    <footer class="about-footer">
      <div class="about-principles" aria-label="产品原则">
        <span v-for="principle in principles" :key="principle">{{ t(principle) }}</span>
      </div>
      <span class="about-stack">Tauri 2 · Vue 3 · Rust</span>
    </footer>
  </el-dialog>
</template>

<style>
.about-dialog {
  border: 1px solid var(--border-primary);
  border-radius: var(--radius-xl);
  overflow: hidden;
}

.about-dialog .el-dialog__header {
  padding: 0;
}

.about-dialog .el-dialog__headerbtn {
  top: 18px;
  right: 18px;
  width: 32px;
  height: 32px;
  border-radius: var(--radius-md);
}

.about-dialog .el-dialog__headerbtn:hover {
  background: var(--bg-hover);
}

.about-dialog .el-dialog__body {
  padding: 0;
}

.about-hero {
  display: grid;
  grid-template-columns: 48px minmax(0, 1fr) auto;
  align-items: start;
  gap: 16px;
  padding: 28px 56px 28px 28px;
  border-bottom: 1px solid var(--border-primary);
  background: var(--bg-primary);
}

.about-brand-mark {
  width: 48px;
  height: 48px;
  display: grid;
  place-items: center;
  border: 1px solid var(--accent-border);
  border-radius: var(--radius-lg);
  background: var(--accent-soft);
  color: var(--accent);
  font-size: 13px;
  font-weight: 750;
  letter-spacing: 0.08em;
}

.about-heading {
  min-width: 0;
}

.about-eyebrow {
  display: block;
  margin: 1px 0 5px;
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.14em;
}

.about-heading h2 {
  margin: 0;
  color: var(--text-primary);
  font-size: 23px;
  font-weight: 700;
  letter-spacing: -0.035em;
}

.about-heading p {
  margin: 7px 0 0;
  color: var(--text-secondary);
  font-size: 13px;
  line-height: 1.55;
}

.about-version {
  margin-top: 2px;
  padding: 4px 8px;
  border: 1px solid var(--border-primary);
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1;
}

.about-section {
  padding: 24px 28px;
}

.about-section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.about-section-head h3 {
  margin: 0;
  color: var(--text-primary);
  font-size: 12px;
  font-weight: 650;
}

.about-section-head > span {
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 9px;
  letter-spacing: 0.1em;
}

.capability-list {
  border-top: 1px solid var(--border-primary);
}

.capability-row {
  display: grid;
  grid-template-columns: 34px 90px minmax(0, 1fr);
  align-items: center;
  min-height: 56px;
  border-bottom: 1px solid var(--border-secondary);
}

.capability-index {
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
}

.capability-row strong {
  color: var(--text-primary);
  font-size: 13px;
  font-weight: 650;
}

.capability-row p {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.5;
}

.about-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 15px 28px;
  border-top: 1px solid var(--border-primary);
  background: var(--bg-secondary);
}

.about-principles {
  display: flex;
  align-items: center;
  gap: 16px;
}

.about-principles span {
  position: relative;
  color: var(--text-secondary);
  font-size: 11px;
}

.about-principles span + span::before {
  content: '';
  position: absolute;
  top: 50%;
  left: -9px;
  width: 2px;
  height: 2px;
  border-radius: 50%;
  background: var(--text-muted);
}

.about-stack {
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
}

@media (max-width: 560px) {
  .about-hero {
    grid-template-columns: 42px minmax(0, 1fr);
    padding: 24px 48px 24px 20px;
  }

  .about-brand-mark {
    width: 42px;
    height: 42px;
  }

  .about-version {
    grid-column: 2;
    justify-self: start;
  }

  .about-section {
    padding: 20px;
  }

  .capability-row {
    grid-template-columns: 30px 1fr;
    padding: 12px 0;
  }

  .capability-row p {
    grid-column: 2;
    margin-top: 4px;
  }

  .about-footer {
    align-items: flex-start;
    flex-direction: column;
    padding: 14px 20px;
  }
}
</style>
