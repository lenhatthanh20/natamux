import type { LucideIcon } from 'lucide-react'
import type { ButtonHTMLAttributes, Ref } from 'react'
import { ICON, type IconButtonSize, iconButton } from './styles'

type IconButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'title' | 'children'> & {
  // The name for screen readers and the tooltip. There is no visible text.
  label: string
  icon: LucideIcon
  // For example 'Mod+B'. Shown in the tooltip.
  shortcut?: string
  // A longer tooltip than the name, for example "Close shell (ends the process)".
  tip?: string
  size?: IconButtonSize
  tipSide?: 'top' | 'bottom'
  ref?: Ref<HTMLButtonElement>
}

export function IconButton({
  label,
  icon: Icon,
  shortcut,
  tip,
  size = 'sm',
  tipSide,
  type = 'button',
  className = '',
  ref,
  ...props
}: IconButtonProps) {
  return (
    <button
      {...props}
      ref={ref}
      type={type}
      aria-label={label}
      data-tip={tip ?? label}
      data-tip-kbd={shortcut}
      data-tip-side={tipSide}
      className={`${iconButton(size)} ${className}`}
    >
      <Icon size={size === 'xs' ? ICON.sm : ICON.md} aria-hidden="true" />
    </button>
  )
}
