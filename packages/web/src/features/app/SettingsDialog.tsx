import type { Repo } from '@natamux/core'
import { Type } from 'lucide-react'
import { type ReactNode, useEffect, useState } from 'react'
import { Button } from '../../components/ui/Button'
import { Dialog } from '../../components/ui/Dialog'
import { Segmented } from '../../components/ui/Segmented'
import type { DesktopSettings } from '../../lib/desktop-bridge'
import { applyTheme, type Theme, useThemeChoice } from '../../lib/theme'
import { toast } from '../../lib/toasts'
import { applyUiSize, UI_SIZE_LABEL, UI_SIZES, useUiSize } from '../../lib/ui-size'
import { TerminalFontDialog } from '../terminals/TerminalFontDialog'

const THEMES: readonly { value: Theme; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
]

type SettingsDialogProps = {
  repos: readonly Repo[]
  onProjectSettings: (repo: Repo) => void
  onAddProject: () => void
  onClose: () => void
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-xs font-medium text-muted">{title}</h3>
      {children}
    </section>
  )
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-h-7 items-center justify-between gap-3 text-sm">
      <span>{label}</span>
      {children}
    </div>
  )
}

// The app-wide settings in one place: the Settings item of the OS menu, the Account menu and Mod+,
// open it. Changes apply at once, like the Account menu.
export function SettingsDialog({
  repos,
  onProjectSettings,
  onAddProject,
  onClose,
}: SettingsDialogProps) {
  const theme = useThemeChoice()
  const size = useUiSize()
  const [pickingFont, setPickingFont] = useState(false)
  const [desktop, setDesktop] = useState<DesktopSettings>()

  useEffect(() => {
    let live = true
    window.natamux
      ?.getDesktopSettings?.()
      .then((settings) => {
        if (live && settings !== undefined) {
          setDesktop(settings)
        }
      })
      .catch(() => undefined)
    return () => {
      live = false
    }
  }, [])

  const changeDesktop = (change: Partial<DesktopSettings>) => {
    const before = desktop
    setDesktop((current) => (current === undefined ? current : { ...current, ...change }))
    window.natamux?.setDesktopSetting?.(change).catch(() => {
      setDesktop(before)
      toast('The setting was not saved.', 'error')
    })
  }

  if (pickingFont) {
    return <TerminalFontDialog onClose={onClose} />
  }

  return (
    <Dialog title="Settings" size="md" onClose={onClose}>
      <div className="flex flex-col gap-5">
        <Section title="Appearance">
          <Row label="Theme">
            <Segmented label="Theme" value={theme} onChange={applyTheme} options={THEMES} />
          </Row>
          <Row label="UI size">
            <Segmented
              label="UI size"
              value={size}
              onChange={applyUiSize}
              options={UI_SIZES.map((value) => ({ value, label: UI_SIZE_LABEL[value] }))}
            />
          </Row>
          <Row label="Terminal">
            <Button icon={Type} onClick={() => setPickingFont(true)}>
              Terminal font…
            </Button>
          </Row>
        </Section>
        {desktop !== undefined && (
          <Section title="Desktop app">
            <label className="flex h-7 items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={desktop.keepAwake}
                onChange={(event) => changeDesktop({ keepAwake: event.target.checked })}
                className="size-3.5 accent-accent"
              />
              Keep computer awake while agents run
            </label>
            {desktop.autoUpdate !== undefined && (
              <label className="flex h-7 items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={desktop.autoUpdate}
                  onChange={(event) => changeDesktop({ autoUpdate: event.target.checked })}
                  className="size-3.5 accent-accent"
                />
                Check for updates automatically
              </label>
            )}
          </Section>
        )}
        <Section title="Projects">
          {repos.length === 0 ? (
            <Row label="No projects yet">
              <Button onClick={onAddProject}>Add a project</Button>
            </Row>
          ) : (
            repos.map((repo) => (
              <Row key={repo.id} label={repo.name}>
                <Button
                  size="xs"
                  variant="ghost"
                  aria-label={`Settings of ${repo.name}`}
                  onClick={() => onProjectSettings(repo)}
                >
                  Settings
                </Button>
              </Row>
            ))
          )}
        </Section>
      </div>
    </Dialog>
  )
}
