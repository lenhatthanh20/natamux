import { useEffect, useRef } from 'react'
import type { CommandActions, CommandState } from '../../lib/commands'
import { findCommand, focusKind, layerIsOpen, publishCommandState } from '../../lib/dispatch'

type KeyboardInput = {
  state: Omit<CommandState, 'focus'>
  actions: CommandActions
}

// The one place where keys become commands (UI spec 5.1). Everything else that listens for keys
// belongs to a control: a field, a menu, a list.
export function useKeyboard(input: KeyboardInput): void {
  const latest = useRef(input)
  latest.current = input
  useEffect(() => publishCommandState({ ...input.state, focus: 'terminal' }))

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // A control that used the key (Escape in a search field) says so with preventDefault.
      if (event.defaultPrevented || event.isComposing || layerIsOpen()) {
        return
      }
      const { state, actions } = latest.current
      const context = { state: { ...state, focus: focusKind(event.target) }, actions }
      const command = findCommand(event, context)
      if (command !== null) {
        event.preventDefault()
        command.run?.(context)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])
}
