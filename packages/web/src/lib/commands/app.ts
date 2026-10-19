import { fireAppEvent } from '../app-events'
import { applyTheme, type Theme } from '../theme'
import { applyUiSize, type UiSize } from '../ui-size'
import type { Command } from './types'

const THEMES: readonly { theme: Theme; title: string }[] = [
  { theme: 'system', title: 'Follow the system' },
  { theme: 'light', title: 'Light' },
  { theme: 'dark', title: 'Dark' },
]
const UI_SIZES: readonly { size: UiSize; title: string }[] = [
  { size: 'compact', title: 'Compact' },
  { size: 'default', title: 'Default' },
  { size: 'large', title: 'Large' },
]

export const globalCommands: Command[] = [
  {
    id: 'palette.open',
    title: 'Open the command palette',
    category: 'App',
    shortcuts: ['Mod+K', 'F1'],
    group: 'Global',
    hidden: true,
    run: ({ actions }) => actions.openPalette(),
  },
  {
    id: 'app.settings',
    title: 'Open the settings',
    category: 'App',
    shortcuts: ['Mod+,'],
    group: 'Global',
    run: ({ actions }) => actions.openSettings(),
  },
  {
    id: 'jump.open',
    title: 'Jump to a workspace',
    category: 'Workspace',
    shortcuts: ['Mod+J'],
    group: 'Global',
    suggested: true,
    run: ({ actions }) => actions.openJump(),
  },
  {
    id: 'sidebar.toggle',
    title: 'Show or hide the sidebar',
    category: 'View',
    shortcuts: ['Mod+B'],
    group: 'Global',
    run: ({ actions }) => actions.toggleSidebar(),
  },
  {
    id: 'details.toggle',
    title: 'Show or hide the details',
    category: 'View',
    shortcuts: ['Mod+Alt+B'],
    group: 'Global',
    run: ({ actions }) => actions.toggleDetails(),
  },
  {
    id: 'focus.next',
    title: 'Move to the next part of the window',
    category: 'View',
    shortcuts: ['F6'],
    group: 'Global',
    hidden: true,
    run: () => fireAppEvent('focus-part', 1),
  },
  {
    id: 'focus.previous',
    title: 'Move to the previous part of the window',
    category: 'View',
    shortcuts: ['Shift+F6'],
    group: 'Global',
    hidden: true,
    run: () => fireAppEvent('focus-part', -1),
  },
  {
    id: 'form.submit',
    title: 'Submit the current form',
    category: 'App',
    shortcuts: ['Mod+Enter'],
    group: 'Global',
    hidden: true,
  },
  {
    id: 'escape',
    title: 'Close the top layer, clear a field, or go back to All tasks',
    category: 'App',
    shortcuts: ['Escape'],
    group: 'Global',
    focus: ['body', 'control', 'field'],
    hidden: true,
    run: ({ actions }) => actions.back(),
  },
]

export const helpCommands: Command[] = [
  {
    id: 'help.shortcuts',
    title: 'Show the keyboard shortcuts',
    category: 'App',
    shortcuts: ['?'],
    group: 'Single keys',
    suggested: true,
    run: ({ actions }) => actions.showShortcuts(),
  },
  {
    id: 'filter.focus',
    title: 'Search the sidebar',
    category: 'View',
    shortcuts: ['/'],
    group: 'Single keys',
    run: ({ actions }) => actions.focusFilter(),
  },
]

export const listMoveCommands: Command[] = [
  {
    id: 'list.next',
    title: 'Select the next task',
    category: 'Task',
    shortcuts: ['j', 'ArrowDown'],
    group: 'Single keys',
    hidden: true,
    when: ({ state }) => !state.workspaceOpen && !state.agentsOpen,
    run: ({ actions }) => actions.moveSelection(1),
  },
  {
    id: 'list.previous',
    title: 'Select the previous task',
    category: 'Task',
    shortcuts: ['k', 'ArrowUp'],
    group: 'Single keys',
    hidden: true,
    when: ({ state }) => !state.workspaceOpen && !state.agentsOpen,
    run: ({ actions }) => actions.moveSelection(-1),
  },
]

export const listKeyCommands: Command[] = [
  {
    id: 'list.ends',
    title: 'First or last row',
    category: 'App',
    shortcuts: ['Home', 'End'],
    group: 'Lists and trees',
    hidden: true,
  },
  {
    id: 'tree.collapse',
    title: 'Collapse or expand a group',
    category: 'App',
    shortcuts: ['ArrowLeft', 'ArrowRight'],
    group: 'Lists and trees',
    hidden: true,
  },
]

export const paletteKeyCommands: Command[] = [
  {
    id: 'palette.move',
    title: 'Move in the list',
    category: 'App',
    shortcuts: ['ArrowUp', 'ArrowDown'],
    group: 'Palette',
    hidden: true,
  },
  {
    id: 'palette.open-row',
    title: 'Open the row, or add it to Agents',
    category: 'App',
    shortcuts: ['Enter', 'Shift+Enter'],
    group: 'Palette',
    hidden: true,
  },
  {
    id: 'palette.digit',
    title: 'Pick row 1 to 9',
    category: 'App',
    shortcuts: ['Alt+1'],
    group: 'Palette',
    hidden: true,
  },
  {
    id: 'palette.scope',
    title: 'Change the scope',
    category: 'App',
    shortcuts: ['Tab'],
    group: 'Palette',
    hidden: true,
  },
]

export const settingsCommands: Command[] = [
  ...THEMES.map(
    ({ theme, title }): Command => ({
      id: `theme.${theme}`,
      title,
      category: 'Theme',
      run: () => applyTheme(theme),
    }),
  ),
  ...UI_SIZES.map(
    ({ size, title }): Command => ({
      id: `ui-size.${size}`,
      title: `Text size: ${title}`,
      category: 'View',
      run: () => applyUiSize(size),
    }),
  ),
  {
    id: 'repo.add',
    title: 'Add a project',
    category: 'Project',
    run: ({ actions }) => actions.openRepos(),
  },
  {
    id: 'repo.import',
    title: 'Import GitHub issues',
    category: 'Project',
    run: ({ actions }) => actions.openImport(),
  },
]
