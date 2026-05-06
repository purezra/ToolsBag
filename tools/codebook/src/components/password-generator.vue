<script setup lang="ts">
import { CopyDocument, Refresh } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import type { PasswordOptions } from '@codebook/types/codebook'
import { useSettings } from '@core/hooks/useSettings'

const props = defineProps<{
  options: PasswordOptions
  generated: string
}>()

const emit = defineEmits<{
  'update:options': [options: PasswordOptions]
  generate: []
}>()

const { t } = useSettings()

const presetLengths = [4, 6, 8, 10, 12, 16, 24, 32]

const updateOption = <K extends keyof PasswordOptions>(key: K, value: PasswordOptions[K]) => {
  emit('update:options', { ...props.options, [key]: value })
}

const copyPassword = async () => {
  if (!props.generated) return
  try {
    await navigator.clipboard.writeText(props.generated)
    ElMessage.success(t('已复制密码'))
  } catch {
    ElMessage.error(t('复制失败'))
  }
}
</script>

<template>
  <div class="password-generator">
    <div class="gen-section">
      <div class="gen-label">{{ t('密码长度') }}</div>
      <div class="length-controls">
        <el-slider
          :model-value="options.length"
          :min="4"
          :max="64"
          :show-tooltip="true"
          style="flex: 1; margin-right: 16px"
          @update:model-value="(v: number | number[]) => updateOption('length', Number(v))"
        />
        <el-input-number
          :model-value="options.length"
          :min="4"
          :max="64"
          size="small"
          style="width: 80px"
          @update:model-value="(v: number | undefined) => updateOption('length', v ?? 16)"
        />
      </div>
      <div class="preset-buttons">
        <el-button
          v-for="len in presetLengths"
          :key="len"
          size="small"
          :type="options.length === len ? 'primary' : 'default'"
          @click="updateOption('length', len)"
        >
          {{ len }}
        </el-button>
      </div>
    </div>

    <div class="gen-section">
      <div class="gen-label">{{ t('字符类型') }}</div>
      <div class="char-options">
        <el-checkbox
          :model-value="options.uppercase"
          @update:model-value="(v: boolean | string | number) => updateOption('uppercase', !!v)"
        >
          A-Z {{ t('大写字母') }}
        </el-checkbox>
        <el-checkbox
          :model-value="options.lowercase"
          @update:model-value="(v: boolean | string | number) => updateOption('lowercase', !!v)"
        >
          a-z {{ t('小写字母') }}
        </el-checkbox>
        <el-checkbox
          :model-value="options.numbers"
          @update:model-value="(v: boolean | string | number) => updateOption('numbers', !!v)"
        >
          0-9 {{ t('数字') }}
        </el-checkbox>
        <el-checkbox
          :model-value="options.symbols"
          @update:model-value="(v: boolean | string | number) => updateOption('symbols', !!v)"
        >
          !@#$%^&* {{ t('特殊符号') }}
        </el-checkbox>
      </div>
    </div>

    <div class="gen-section">
      <div class="gen-label">{{ t('高级规则') }}</div>
      <div class="advanced-rules">
        <el-checkbox
          :model-value="options.excludeAmbiguous"
          @update:model-value="(v: boolean | string | number) => updateOption('excludeAmbiguous', !!v)"
        >
          {{ t('排除易混淆字符') }} (1, l, L, 0, O)
        </el-checkbox>
        <el-checkbox
          :model-value="options.excludeCodeSymbols"
          @update:model-value="(v: boolean | string | number) => updateOption('excludeCodeSymbols', !!v)"
        >
          {{ t('排除代码特殊符') }} ({}, [], (), /, \, ')
        </el-checkbox>
        <div class="custom-exclude">
          <span>{{ t('自定义排除') }}：</span>
          <el-input
            :model-value="options.customExclude"
            size="small"
            style="width: 200px"
            :placeholder="t('输入要排除的字符')"
            @update:model-value="(v: string) => updateOption('customExclude', v)"
          />
        </div>
      </div>
    </div>

    <div class="gen-section">
      <div class="gen-label">{{ t('生成结果') }}</div>
      <div class="result-area">
        <el-input
          :model-value="generated"
          readonly
          size="large"
          class="password-display"
          :placeholder="t('点击生成密码')"
        />
        <el-button type="primary" :icon="Refresh" @click="emit('generate')">
          {{ t('生成密码') }}
        </el-button>
        <el-button :icon="CopyDocument" :disabled="!generated" @click="copyPassword">
          {{ t('复制') }}
        </el-button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.password-generator {
  padding: 16px;
}
.gen-section {
  margin-bottom: 20px;
}
.gen-label {
  font-weight: 600;
  margin-bottom: 8px;
  color: var(--el-text-color-primary);
}
.length-controls {
  display: flex;
  align-items: center;
}
.preset-buttons {
  display: flex;
  gap: 8px;
  margin-top: 8px;
  flex-wrap: wrap;
}
.char-options {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
}
.advanced-rules {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.custom-exclude {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 4px;
}
.result-area {
  display: flex;
  gap: 8px;
  align-items: center;
}
.password-display {
  flex: 1;
}
.password-display :deep(.el-input__inner) {
  font-family: 'JetBrains Mono', 'Consolas', monospace !important;
  font-size: 15px;
  letter-spacing: 0.5px;
}
</style>
