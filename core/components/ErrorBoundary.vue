<script setup lang="ts">
import { ref, watch, onErrorCaptured } from 'vue'

const props = defineProps<{
  keyName?: string
}>()

const error = ref<Error | null>(null)

const reset = () => {
  error.value = null
}

// reset when key changes (different tool)
watch(() => props.keyName, () => reset())

defineExpose({ reset })

// Vue errorCaptured lifecycle hook - captures errors from child components
onErrorCaptured((err: unknown) => {
  if (err instanceof Error) {
    error.value = err
  } else {
    error.value = new Error(String(err))
  }
  // prevent error from propagating to parent
  return false
})
</script>

<template>
  <div v-if="error" class="error-boundary">
    <p class="error-title">组件渲染异常</p>
    <p class="error-message">{{ error.message }}</p>
    <el-button type="primary" size="small" @click="reset">重试</el-button>
  </div>
  <div v-else>
    <slot />
  </div>
</template>

<style scoped>
.error-boundary {
  padding: 20px;
  border: 1px solid var(--el-border-color);
  border-radius: 8px;
  background: var(--el-fill-color-light);
  color: var(--el-color-danger);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.error-title {
  margin: 0;
  font-weight: 700;
}
.error-message {
  margin: 0;
  color: var(--el-text-color-primary);
}
</style>
