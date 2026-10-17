import { Bug, CircleUser, Keyboard, LogOut, Type } from 'lucide-react'
import { useState } from 'react'
import { shortcutOf } from '../lib/commands'
import { applyTheme, type Theme, useThemeChoice } from '../lib/theme'
import { applyUiSize, UI_SIZE_LABEL, UI_SIZES, useUiSize } from '../lib/ui-size'
import { TerminalFontDialog } from './TerminalFontDialog'
import { ConfirmDialog } from './ui/Dialog'
import { Menu } from './ui/Menu'

const THEMES: readonly { value: Theme; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
]

const ISSUES_URL = 'https://github.com/lenhatthanh20/natamux/issues/new'

type AccountMenuProps = {
  onSignOut: () => void
  onShortcuts: () => void
}

// Theme, UI size, terminal font, shortcuts, issue reporting and sign out in one menu. Signing out
// asks first.
export function AccountMenu({ onSignOut, onShortcuts }: AccountMenuProps) {
  const theme = useThemeChoice()
  const size = useUiSize()
  const [asking, setAsking] = useState(false)
  const [pickingFont, setPickingFont] = useState(false)

  return (
    <>
      <Menu
        label="Account"
        icon={CircleUser}
        tipSide="top"
        items={[
          { type: 'label', label: 'Theme' },
          ...THEMES.map(({ value, label }) => ({
            label,
            checked: theme === value,
            onSelect: () => applyTheme(value),
          })),
          { type: 'separator' },
          { type: 'label', label: 'UI size' },
          ...UI_SIZES.map((value) => ({
            label: UI_SIZE_LABEL[value],
            checked: size === value,
            onSelect: () => applyUiSize(value),
          })),
          { label: 'Terminal font…', icon: Type, onSelect: () => setPickingFont(true) },
          { type: 'separator' },
          {
            label: 'Keyboard shortcuts',
            icon: Keyboard,
            shortcut: shortcutOf('help.shortcuts'),
            onSelect: onShortcuts,
          },
          {
            label: 'Report an issue',
            icon: Bug,
            onSelect: () => window.open(ISSUES_URL, '_blank', 'noopener,noreferrer'),
          },
          { type: 'separator' },
          { label: 'Sign out', icon: LogOut, danger: true, onSelect: () => setAsking(true) },
        ]}
      />
      {pickingFont && <TerminalFontDialog onClose={() => setPickingFont(false)} />}
      {asking && (
        <ConfirmDialog
          title="Sign out of Natamux?"
          description="Your tasks keep running."
          confirmLabel="Sign out"
          onConfirm={onSignOut}
          onClose={() => setAsking(false)}
        />
      )}
    </>
  )
}
