import type { ISearchOptions } from '@xterm/addon-search'
import type { ITerminalOptions, ITheme } from '@xterm/xterm'
import type { ResolvedTheme } from '../../lib/theme'

// xterm needs real colors, not CSS variables. Values match the terminal palette in the UI spec.
// The "Night" palette, picked from four designs in docs/design-terminal-look.md.
const DARK: ITheme = {
  background: '#0f1219',
  foreground: '#d6dcf0',
  cursor: '#7aa2ff',
  cursorAccent: '#0f1219',
  selectionBackground: '#7aa2ff47',
  selectionInactiveBackground: '#7aa2ff21',
  scrollbarSliderBackground: '#ffffff24',
  scrollbarSliderHoverBackground: '#ffffff42',
  scrollbarSliderActiveBackground: '#ffffff5c',
  // Unset, xterm draws a light line left of the scrollbar.
  overviewRulerBorder: '#00000000',
  black: '#343b52',
  brightBlack: '#7c86a8',
  red: '#f7768e',
  brightRed: '#ff9bae',
  green: '#8fd694',
  brightGreen: '#aee6b2',
  yellow: '#e6c07b',
  brightYellow: '#f0d39d',
  blue: '#7aa2ff',
  brightBlue: '#a6c1ff',
  magenta: '#bb9af7',
  brightMagenta: '#d2bbfa',
  cyan: '#6fd0e0',
  brightCyan: '#9ce0eb',
  white: '#b8c0dc',
  brightWhite: '#e8ecf8',
}

const LIGHT: ITheme = {
  background: '#f7f9fd',
  foreground: '#1b2238',
  cursor: '#2f5fe0',
  cursorAccent: '#f7f9fd',
  selectionBackground: '#2f5fe030',
  selectionInactiveBackground: '#2f5fe018',
  scrollbarSliderBackground: '#14181f33',
  scrollbarSliderHoverBackground: '#14181f5c',
  scrollbarSliderActiveBackground: '#14181f7a',
  overviewRulerBorder: '#00000000',
  black: '#262d45',
  brightBlack: '#5d6684',
  red: '#c4304f',
  brightRed: '#b02a46',
  green: '#2c7a3a',
  brightGreen: '#256c33',
  yellow: '#8c5f00',
  brightYellow: '#7b5300',
  blue: '#2f5fe0',
  brightBlue: '#2448b8',
  magenta: '#7c4dd6',
  brightMagenta: '#6a3ec2',
  cyan: '#0b6a80',
  brightCyan: '#095c6f',
  white: '#5d6684',
  brightWhite: '#1b2238',
}

export function terminalTheme(theme: ResolvedTheme): ITheme {
  return { ...(theme === 'dark' ? DARK : LIGHT) }
}

// Any program color is pushed to a readable one on the terminal background.
export const MIN_CONTRAST_RATIO = 4.5
export const TERMINAL_FONT_SIZE = 13
// xterm multiplies the font's own line height (about 1.3 em for JetBrains Mono), not the font size.
// 1 gives the rows of Ghostty and VS Code: about 17px at 13px.
export const TERMINAL_LINE_HEIGHT = 1
// xterm sizes its scrollbar by the overview ruler width, so the ruler sets it. 8px matches
// the code editor's scrollbar.
export const TERMINAL_SCROLLBAR_WIDTH = 8

// The look of every terminal, close to Ghostty and VS Code. Ligatures, transparency and smooth
// scrolling stay off: each one costs drawing time.
export const TERMINAL_LOOK = {
  cursorBlink: true,
  cursorStyle: 'block',
  // An unfocused terminal shows where its cursor is without looking like it takes keys.
  cursorInactiveStyle: 'outline',
  // Bold text keeps its color and only gets heavier, as in most modern themes.
  drawBoldTextInBrightColors: false,
  // The weights of Ghostty, VS Code and JetBrains IDEs.
  fontWeight: 'normal',
  fontWeightBold: 'bold',
  lineHeight: TERMINAL_LINE_HEIGHT,
  minimumContrastRatio: MIN_CONTRAST_RATIO,
  fastScrollSensitivity: 5,
  overviewRuler: { width: TERMINAL_SCROLLBAR_WIDTH },
} satisfies ITerminalOptions

type FindDecorations = NonNullable<ISearchOptions['decorations']>

// The search addon only counts matches when it draws them, and wants #RRGGBB colors.
export function findDecorations(theme: ResolvedTheme): FindDecorations {
  return theme === 'dark'
    ? {
        matchBackground: '#4d4220',
        matchOverviewRuler: '#e8c15a',
        activeMatchBackground: '#8a6a12',
        activeMatchBorder: '#f5d47a',
        activeMatchColorOverviewRuler: '#f5d47a',
      }
    : {
        matchBackground: '#f6e7a8',
        matchOverviewRuler: '#8a5a00',
        activeMatchBackground: '#f0c040',
        activeMatchBorder: '#8a5a00',
        activeMatchColorOverviewRuler: '#8a5a00',
      }
}
