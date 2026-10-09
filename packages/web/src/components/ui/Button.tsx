import { LoaderCircle, type LucideIcon } from 'lucide-react'
import {
  type ButtonHTMLAttributes,
  type MouseEvent,
  type Ref,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'
import { Kbd } from './Kbd'
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
  const [minWidth, setMinWidth] = useState<number | undefined>(undefined)
  const blocked = disabledReason !== undefined

  // The first width is the idle one, so the label does not jump when the spinner appears.
  useLayoutEffect(() => {
    if (minWidth === undefined && own.current) {
      setMinWidth(own.current.offsetWidth)
    }
  }, [minWidth])

  const click = (event: MouseEvent<HTMLButtonElement>) => {
    if (blocked) {
      event.preventDefault()
      return
    }
    onClick?.(event)
  }

  const ShownIcon = busy ? LoaderCircle : Icon
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
      style={{ ...props.style, minWidth }}
      onClick={click}
      className={`${button(variant, size)} ${className}`}
    >
      {ShownIcon !== undefined && (
        <ShownIcon
          size={size === 'xs' ? ICON.xs : ICON.sm}
          aria-hidden="true"
          className={busy ? 'animate-spin motion-reduce:animate-none' : ''}
        />
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
