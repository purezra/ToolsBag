export const splitPatterns = (raw: string): string[] => {
  return raw
    .split(/[,，]/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
}
