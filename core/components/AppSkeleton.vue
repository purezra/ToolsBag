<script setup lang="ts">
interface Props {
  rows?: number
  animated?: boolean
  variant?: 'card' | 'table' | 'form' | 'list'
}

withDefaults(defineProps<Props>(), {
  rows: 5,
  animated: true,
  variant: 'card'
})
</script>

<template>
  <div :class="['app-skeleton', `app-skeleton--${variant}`]">
    <!-- Card variant -->
    <template v-if="variant === 'card'">
      <div class="skeleton-header">
        <div class="skeleton-line skeleton-title" />
        <div class="skeleton-line skeleton-subtitle" />
      </div>
      <div class="skeleton-body">
        <div
          v-for="i in rows"
          :key="i"
          class="skeleton-line"
          :style="{ width: `${60 + Math.random() * 40}%` }"
        />
      </div>
    </template>

    <!-- Table variant -->
    <template v-else-if="variant === 'table'">
      <div class="skeleton-table-header">
        <div v-for="j in 4" :key="j" class="skeleton-th" />
      </div>
      <div
        v-for="i in rows"
        :key="i"
        class="skeleton-table-row"
      >
        <div v-for="j in 4" :key="j" class="skeleton-td" />
      </div>
    </template>

    <!-- Form variant -->
    <template v-else-if="variant === 'form'">
      <div
        v-for="i in rows"
        :key="i"
        class="skeleton-form-item"
      >
        <div class="skeleton-line skeleton-label" />
        <div class="skeleton-line skeleton-input" />
      </div>
    </template>

    <!-- List variant -->
    <template v-else>
      <div
        v-for="i in rows"
        :key="i"
        class="skeleton-list-item"
      >
        <div class="skeleton-avatar" />
        <div class="skeleton-list-content">
          <div class="skeleton-line skeleton-list-title" />
          <div class="skeleton-line skeleton-list-desc" />
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.app-skeleton {
  padding: var(--card-padding, 24px);
}

.skeleton-line {
  height: 14px;
  border-radius: var(--radius-sm, 8px);
  background: linear-gradient(
    90deg,
    var(--bg-tertiary) 25%,
    var(--bg-secondary) 50%,
    var(--bg-tertiary) 75%
  );
  background-size: 200% 100%;
  animation: skeleton-shimmer 1.5s ease-in-out infinite;
}

.app-skeleton:not(.app-skeleton--animated) .skeleton-line,
.app-skeleton[animated="false"] .skeleton-line {
  animation: none;
}

@keyframes skeleton-shimmer {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}

/* Header */
.skeleton-header {
  margin-bottom: 20px;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--border-secondary);
}

.skeleton-title {
  width: 40%;
  height: 20px;
  margin-bottom: 8px;
}

.skeleton-subtitle {
  width: 25%;
  height: 12px;
}

/* Body */
.skeleton-body {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

/* Table */
.skeleton-table-header {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
  padding: 12px 0;
  border-bottom: 2px solid var(--border-secondary);
  margin-bottom: 8px;
}

.skeleton-th {
  height: 12px;
  border-radius: var(--radius-xs, 6px);
  background: var(--bg-tertiary);
  opacity: 0.6;
}

.skeleton-table-row {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
  padding: 14px 0;
  border-bottom: 1px solid var(--border-secondary);
}

.skeleton-td {
  height: 14px;
  border-radius: var(--radius-xs, 6px);
  animation: skeleton-shimmer 1.5s ease-in-out infinite;
  background: linear-gradient(
    90deg,
    var(--bg-tertiary) 25%,
    var(--bg-secondary) 50%,
    var(--bg-tertiary) 75%
  );
  background-size: 200% 100%;
}

/* Form */
.skeleton-form-item {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 20px;
}

.skeleton-label {
  width: 100px;
  height: 12px;
}

.skeleton-input {
  width: 100%;
  height: 40px;
  border-radius: var(--radius-sm, 8px);
}

/* List */
.skeleton-list-item {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 12px 0;
  border-bottom: 1px solid var(--border-secondary);
}

.skeleton-avatar {
  width: 40px;
  height: 40px;
  border-radius: var(--radius-pill, 999px);
  background: linear-gradient(
    90deg,
    var(--bg-tertiary) 25%,
    var(--bg-secondary) 50%,
    var(--bg-tertiary) 75%
  );
  background-size: 200% 100%;
  animation: skeleton-shimmer 1.5s ease-in-out infinite;
  flex-shrink: 0;
}

.skeleton-list-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.skeleton-list-title {
  width: 60%;
}

.skeleton-list-desc {
  width: 80%;
  height: 12px;
}
</style>
