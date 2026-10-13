// The same stack as --font-mono in styles.css, for small mono labels in the UI.
export const MONO_FONT =
  '"Geist Mono Variable", ui-monospace, "SF Mono", "Cascadia Mono", Menlo, Consolas, monospace'

// Nerd Font icons for prompts and file lists, as Ghostty bundles them. Declared in styles.css.
export const ICON_FONT = 'Symbols Nerd Font Mono'

// Terminals and the code views (Monaco) use JetBrains Mono, the font of most native terminals, so
// code looks the same in a diff and in a shell. The icon font comes right after it, so it also
// fills the icons of a font the user picks (fontStack puts that one first).
export const TERMINAL_FONT = `"JetBrains Mono Variable", "${ICON_FONT}", ${MONO_FONT}`
