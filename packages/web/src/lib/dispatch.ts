import { isMac } from './keys'
import {
  COMMANDS,
  type Command,
  type CommandContext,
  type CommandState,
  type FocusKind,
  runnable,
} from './commands'
import {
  isSingleKey,
  matchesShortcut,
  reachesAppFromTerminal,
  type ShortcutEvent,
} from './shortcut'

const TYPING_TAGS = ['INPUT', 'TEXTAREA', 'SELECT']

export function focusKind(target: EventTarget | null): FocusKind {
  if (!(target instanceof Element)) {
    return 'body'
  }
  if (target.closest('.xterm') !== null) {
    return 'terminal'
  }
  // Monaco has its own key handling, and its text area is a field too.
  if (
    target.closest('.monaco-editor') !== null ||
    TYPING_TAGS.includes(target.tagName) ||
    (target instanceof HTMLElement && target.isContentEditable)
  ) {
    return 'field'
  }
  if (target === document.body || target.id === 'main' || target === document.documentElement) {
    return 'body'
  }
  return 'control'
}

// A dialog, a menu or a palette is open: it owns the keyboard.
export function layerIsOpen(): boolean {
  return document.querySelector('[role="dialog"], [role="menu"]') !== null
}

function defaultFocus(shortcut: string): readonly FocusKind[] {
  if (isSingleKey(shortcut)) {
    return ['body', 'control']
  }
  return ['body', 'control', 'field', 'terminal']
}

// Whether this shortcut of this command may fire with the focus where it is. Single keys type
// characters, so fields and terminals keep them. In a terminal only the keys of section 5.1 of
// the UI spec reach the app.
export function mayFire(
  command: Command,
  shortcut: string,
  focus: FocusKind,
  mac = isMac(),
): boolean {
  const allowed = command.focus ?? defaultFocus(shortcut)
  if (!allowed.includes(focus)) {
    return false
  }
  if (focus === 'terminal') {
    return reachesAppFromTerminal(shortcut, mac)
  }
  return true
}

export function findCommand(
  event: ShortcutEvent,
  context: CommandContext,
  mac = isMac(),
  commands: readonly Command[] = COMMANDS,
): Command | null {
  for (const command of commands) {
    if (command.run === undefined || command.shortcuts === undefined) {
      continue
    }
    for (const shortcut of command.shortcuts) {
      if (
        matchesShortcut(event, shortcut, mac) &&
        mayFire(command, shortcut, context.state.focus, mac) &&
        runnable(command, context)
      ) {
        return command
      }
    }
  }
  return null
}

// The state that commands read in `when`. The dashboard publishes it on every render, so the
// terminal can ask about a key without a React context.
let published: CommandState = {
  focus: 'terminal',
  workspaceOpen: false,
  agentsOpen: false,
  hasSelection: false,
  selectedState: null,
}

export function publishCommandState(state: CommandState): void {
  published = state
}

// The terminal only asks whether a key is an app key, so the commands never run from here.
const NO_ACTIONS = new Proxy({} as CommandContext['actions'], {
  get: () => () => undefined,
})

// What xterm asks for every key: false means "not for the shell", and the key goes on to the
// page, where the dispatcher runs the command.
export function keyBelongsToShell(
  event: ShortcutEvent,
  mac = isMac(),
  commands: readonly Command[] = COMMANDS,
): boolean {
  const context = { state: { ...published, focus: 'terminal' as const }, actions: NO_ACTIONS }
  return findCommand(event, context, mac, commands) === null
}
