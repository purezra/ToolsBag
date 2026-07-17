export const splitPatterns = (raw: string): string[] => {
  return raw
    .split(/[,，]/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
}

export const hasSelectedFormats = (selected: Record<string, string[]>): boolean =>
  Object.values(selected).some((formats) => formats.length > 0)
