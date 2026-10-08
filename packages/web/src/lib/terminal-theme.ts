import type { ITheme } from '@xterm/xterm'
import type { ResolvedTheme } from './theme'

// xterm needs real colors, not CSS variables. Values match the terminal palette in the UI spec.
const DARK: ITheme = {
  background: '#0b0c0e',
  foreground: '#e6e7ea',
  cursor: '#7aa2ff',
  cursorAccent: '#0b0c0e',
  selectionBackground: '#7aa2ff4d',
  black: '#3a3f4b',
  brightBlack: '#7d828c',
  red: '#ff7b72',
  brightRed: '#ffa198',
  green: '#6fd19a',
  brightGreen: '#8fe0b0',
  yellow: '#e8c15a',
  brightYellow: '#f5d47a',
  blue: '#7aa2ff',
  brightBlue: '#a5c0ff',
  magenta: '#d09bff',
  brightMagenta: '#e2bcff',
  cyan: '#6cc4d6',
  brightCyan: '#97dbe8',
  white: '#c9ccd2',
  brightWhite: '#f2f3f5',
}

const LIGHT: ITheme = {
  background: '#fbfbfc',
  foreground: '#17191d',
  cursor: '#2f5fe0',
  cursorAccent: '#fbfbfc',
  selectionBackground: '#2f5fe033',
  black: '#24272d',
  brightBlack: '#5f6570',
  red: '#c0362c',
  brightRed: '#b93a30',
  green: '#1c7442',
  brightGreen: '#237a48',
  yellow: '#8a5a00',
  brightYellow: '#8f6000',
  blue: '#2448b8',
  brightBlue: '#2f5fe0',
  magenta: '#8a3fb8',
  brightMagenta: '#8f45c2',
  cyan: '#0b6274',
  brightCyan: '#0f7489',
  white: '#5f6570',
  brightWhite: '#17191d',
}

export function terminalTheme(theme: ResolvedTheme): ITheme {
  return { ...(theme === 'dark' ? DARK : LIGHT) }
}

// Any program color is pushed to a readable one on the terminal background.
export const MIN_CONTRAST_RATIO = 4.5
export const TERMINAL_FONT_SIZE = 13
export const TERMINAL_LINE_HEIGHT = 1.3
