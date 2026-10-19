export const SUMMARY_LENGTH = 80

export function shorten(text: string, length = SUMMARY_LENGTH): string {
  const line = text.replace(/\s+/g, ' ').trim()
  return line.length > length ? `${line.slice(0, length - 1)}…` : line
}
