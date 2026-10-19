import {
  globalCommands,
  helpCommands,
  listKeyCommands,
  listMoveCommands,
  paletteKeyCommands,
  settingsCommands,
} from './app'
import {
  rowCommands,
  stageCommands,
  startCommands,
  taskActionCommands,
  taskKeyCommands,
} from './task'
import { type Command, type CommandContext, SHEET_GROUPS, type SheetGroup } from './types'
import {
  diffCommands,
  paneCommands,
  tabCommands,
  viewCommands,
  workspaceViewCommands,
} from './view'

export type {
  Command,
  CommandActions,
  CommandCategory,
  CommandContext,
  CommandState,
  FocusKind,
  SheetGroup,
} from './types'
export { SHEET_GROUPS } from './types'

export const COMMANDS: readonly Command[] = [
  // Global
  ...globalCommands,
  // Single keys
  ...helpCommands,
  ...startCommands,
  ...listMoveCommands,
  ...taskKeyCommands,
  ...taskActionCommands,
  ...tabCommands,
  ...stageCommands,
  ...workspaceViewCommands,
  // Lists and trees: the controls handle these keys themselves.
  ...listKeyCommands,
  ...rowCommands,
  // Panes
  ...paneCommands,
  // Diff
  ...diffCommands,
  // The palette's own keys
  ...paletteKeyCommands,
  // Only in the palette
  ...viewCommands,
  ...settingsCommands,
]

export function commandById(id: string): Command | undefined {
  return COMMANDS.find((command) => command.id === id)
}

// The first shortcut of a command, for a tooltip or a menu row.
export function shortcutOf(id: string): string | undefined {
  return commandById(id)?.shortcuts?.[0]
}

// The title of a command in the palette: "Task: Approve".
export function paletteTitle(command: Command): string {
  return `${command.category}: ${command.title}`
}

export function sheetGroups(
  commands: readonly Command[] = COMMANDS,
): { group: SheetGroup; commands: Command[] }[] {
  return SHEET_GROUPS.map((group) => ({
    group,
    commands: commands.filter((command) => command.group === group),
  })).filter(({ commands: inGroup }) => inGroup.length > 0)
}

export function runnable(command: Command, context: CommandContext): boolean {
  return command.run !== undefined && (command.when?.(context) ?? true)
}
