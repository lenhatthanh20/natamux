import { CircleUser, Keyboard, LogOut } from 'lucide-react'
import { useState } from 'react'
import { shortcutOf } from '../lib/commands'
import { applyTheme, type Theme, useThemeChoice } from '../lib/theme'
import { applyUiSize, UI_SIZE_LABEL, UI_SIZES, useUiSize } from '../lib/ui-size'
import { ConfirmDialog } from './ui/Dialog'
import { Menu } from './ui/Menu'

const THEMES: readonly { value: Theme; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
]

type AccountMenuProps = {
  onSignOut: () => void
  onShortcuts: () => void
}

// Theme, UI size, shortcuts and sign out in one menu. Signing out asks first.
export function AccountMenu({ onSignOut, onShortcuts }: AccountMenuProps) {
  const theme = useThemeChoice()
  const size = useUiSize()
  const [asking, setAsking] = useState(false)

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
          { type: 'separator' },
          {
            label: 'Keyboard shortcuts',
            icon: Keyboard,
            shortcut: shortcutOf('help.shortcuts'),
            onSelect: onShortcuts,
          },
          { type: 'separator' },
          { label: 'Sign out', icon: LogOut, danger: true, onSelect: () => setAsking(true) },
        ]}
      />
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
