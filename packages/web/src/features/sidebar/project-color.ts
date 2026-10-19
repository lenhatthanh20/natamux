// The --project-0 … --project-5 tokens in styles.css.
export const PROJECT_COLORS = 6

// A project's color comes from its id, so it stays the same across reloads with nothing stored.
export function projectColor(id: string): string {
  let hash = 0
  for (const char of id) {
    hash = (hash * 31 + char.charCodeAt(0)) >>> 0
  }
  return `var(--project-${hash % PROJECT_COLORS})`
}
