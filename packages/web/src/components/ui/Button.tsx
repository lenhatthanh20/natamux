import type { LucideIcon } from 'lucide-react'
import {
  type ButtonHTMLAttributes,
  type MouseEvent,
  type Ref,
  useLayoutEffect,
  useRef,
} from 'react'
import { Kbd } from './Kbd'
import { Spinner } from './Spinner'
import { type ButtonSize, type ButtonVariant, button, ICON } from './styles'

type ButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'title'> & {
  variant?: ButtonVariant
  size?: ButtonSize
  icon?: LucideIcon
  // The label stays and a spinner takes the icon's place. The button keeps its width.
  busy?: boolean
  // Says why the button does nothing. It stays focusable, so the reason can be read.
  disabledReason?: string
  // For example 'Mod+Enter'. Shown at the end of the label.
  shortcut?: string
  ref?: Ref<HTMLButtonElement>
}

export function Button({
  variant = 'secondary',
  size = 'sm',
  icon: Icon,
  busy = false,
  disabledReason,
  shortcut,
  type = 'button',
  className = '',
  onClick,
  children,
  ref,
  disabled,
  ...props
}: ButtonProps) {
  const own = useRef<HTMLButtonElement | null>(null)
  const idleWidth = useRef<number | undefined>(undefined)
  const blocked = disabledReason !== undefined

  // The width is kept only while busy, so the button does not jump when the spinner appears. It is
  // measured again when the label changes, so a shorter label also gets a narrower button.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the label parts are the triggers.
  useLayoutEffect(() => {
    if (!busy && own.current) {
      idleWidth.current = own.current.offsetWidth
    }
  }, [busy, children, Icon, shortcut, size])

  const iconSize = size === 'xs' ? ICON.xs : ICON.sm

  const click = (event: MouseEvent<HTMLButtonElement>) => {
    if (blocked) {
      event.preventDefault()
      return
    }
    onClick?.(event)
  }

  return (
    <button
      {...props}
      ref={(node) => {
        own.current = node
        if (typeof ref === 'function') {
          ref(node)
        } else if (ref) {
          ref.current = node
        }
      }}
      type={type}
      disabled={disabled === true || busy}
      aria-disabled={blocked ? true : undefined}
      aria-busy={busy ? true : undefined}
      data-variant={variant}
      {...(blocked ? { 'data-tip': disabledReason } : {})}
      style={busy ? { ...props.style, minWidth: idleWidth.current } : props.style}
      onClick={click}
      className={`${button(variant, size)} ${className}`}
    >
      {busy ? (
        <Spinner size={iconSize} />
      ) : (
        Icon !== undefined && <Icon size={iconSize} aria-hidden="true" />
      )}
      {children}
      {shortcut !== undefined && (
        <span aria-hidden="true" className="ml-1 inline-flex">
          <Kbd shortcut={shortcut} onInverted={variant === 'primary' || variant === 'danger'} />
        </span>
      )}
    </button>
  )
}
