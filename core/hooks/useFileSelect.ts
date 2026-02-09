import { ElMessage } from 'element-plus'
import { readClipboardPaths, selectPaths } from '../api/common'
import type { PickKind } from '../api/common'

export const useFileSelect = () => {
  const pick = async (kind: PickKind): Promise<string[]> => {
    try {
      if (kind === 'clipboard') {
        return await readClipboardPaths()
      }
      return await selectPaths(kind)
    } catch (err: any) {
      ElMessage.error(err?.toString() || '路径选择失败')
      return []
    }
  }

  return { pick }
}
