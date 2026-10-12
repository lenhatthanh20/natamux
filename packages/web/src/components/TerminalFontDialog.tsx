import { useState } from 'react'
import {
  fontStack,
  ghosttyFont,
  saveTerminalFont,
  TERMINAL_FONT_MAX_SIZE,
  TERMINAL_FONT_MIN_SIZE,
  terminalFont,
} from '../lib/terminal-font'
import { TERMINAL_FONT_SIZE } from '../lib/terminal-theme'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { input } from './ui/styles'

const FIELD_LABEL = 'text-xs font-medium text-muted'

type TerminalFontDialogProps = {
  onClose: () => void
}

// The font of every terminal. Empty fields use Ghostty's font when the desktop app found one,
// else the built-in font.
export function TerminalFontDialog({ onClose }: TerminalFontDialogProps) {
  const current = terminalFont()
  const ghostty = ghosttyFont()
  const saved = current.source === 'custom' ? current.saved : undefined
  const [family, setFamily] = useState(saved?.family ?? '')
  const [size, setSize] = useState(saved?.size === undefined ? '' : String(saved.size))
  const fallbackFamily = ghostty?.family ?? 'JetBrains Mono'
  const fallbackSize = ghostty?.size ?? TERMINAL_FONT_SIZE
  const sizeNumber = size.trim() === '' ? fallbackSize : Number(size)
  const sizeValid =
    Number.isFinite(sizeNumber) &&
    sizeNumber >= TERMINAL_FONT_MIN_SIZE &&
    sizeNumber <= TERMINAL_FONT_MAX_SIZE

  const save = () => {
    if (!sizeValid) {
      return
    }
    const name = family.trim()
    const font = {
      ...(name === '' ? {} : { family: name }),
      ...(size.trim() === '' ? {} : { size: sizeNumber }),
    }
    saveTerminalFont(Object.keys(font).length === 0 ? null : font)
    onClose()
  }

  return (
    <Dialog
      title="Terminal font"
      description={
        ghostty
          ? `Empty fields use your Ghostty font: ${fallbackFamily}, ${fallbackSize}px.`
          : `Empty fields use the built-in font, ${fallbackFamily}, ${fallbackSize}px. A font that is not installed falls back to it.`
      }
      size="sm"
      onClose={onClose}
      onSubmit={save}
      footer={
        <>
          {current.source === 'custom' && (
            <Button
              variant="ghost"
              onClick={() => {
                saveTerminalFont(null)
                onClose()
              }}
            >
              {ghostty ? 'Use Ghostty font' : 'Use built-in font'}
            </Button>
          )}
          <span className="flex-1" />
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            shortcut="Mod+Enter"
            onClick={save}
            {...(sizeValid ? {} : { disabledReason: 'Pick a size from 8 to 32' })}
          >
            Save
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <label className="flex flex-col gap-1">
          <span className={FIELD_LABEL}>Font family</span>
          <input
            value={family}
            onChange={(event) => setFamily(event.target.value)}
            placeholder={fallbackFamily}
            spellCheck={false}
            data-autofocus
            className={input}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className={FIELD_LABEL}>Size (px)</span>
          <input
            value={size}
            onChange={(event) => setSize(event.target.value)}
            placeholder={String(fallbackSize)}
            inputMode="decimal"
            aria-invalid={!sizeValid}
            className={`${input} w-24`}
          />
        </label>
        <p
          className="rounded-md border border-line px-3 py-2 text-sm"
          style={{
            fontFamily: fontStack(family.trim() || ghostty?.family),
            fontSize: sizeValid ? `${sizeNumber}px` : undefined,
          }}
        >
          {'git commit -m "Fix café → naïve" ✓ │ 0O 1lI {}'}
        </p>
      </div>
    </Dialog>
  )
}
