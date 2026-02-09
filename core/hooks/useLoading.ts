import { ref } from 'vue'

export const useLoading = () => {
  const loading = ref(false)

  const runWithLoading = async <T>(fn: () => Promise<T>): Promise<T | undefined> => {
    loading.value = true
    try {
      return await fn()
    } finally {
      loading.value = false
    }
  }

  return { loading, runWithLoading }
}
