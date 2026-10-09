import { useLayoutEffect, useRef, useState } from 'react'

type MiddleTruncateProps = {
  text: string
  // How many characters stay visible at the end when the text is cut.
  tail?: number
}

// One unbroken text when it fits. Only when it is cut does it show the beginning, "…" and the end.
// A hidden copy of the whole text tells whether it fits, so the cut view never decides that.
export function MiddleTruncate({ text, tail = 10 }: MiddleTruncateProps) {
  const box = useRef<HTMLSpanElement>(null)
  const probe = useRef<HTMLSpanElement>(null)
  const [cut, setCut] = useState(false)

  // biome-ignore lint/correctness/useExhaustiveDependencies: the text changes the width to measure.
  useLayoutEffect(() => {
    const element = box.current
    if (element === null) {
      return
    }
    const measure = () => {
      const full = probe.current?.getBoundingClientRect().width ?? 0
      setCut(full - element.getBoundingClientRect().width > 0.5)
    }
    measure()
    if (typeof ResizeObserver === 'undefined') {
      return
    }
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [text])

  const split = cut && text.length > tail
  return (
    <span ref={box} className="relative flex min-w-0 overflow-hidden whitespace-nowrap">
      <span
        ref={probe}
        aria-hidden="true"
        className="pointer-events-none invisible absolute top-0 left-0 whitespace-nowrap"
      >
        {text}
      </span>
      {split ? (
        <>
          <span className="min-w-0 truncate">{text.slice(0, -tail)}</span>
          <span className="shrink-0">{text.slice(-tail)}</span>
        </>
      ) : (
        <span className="min-w-0 truncate">{text}</span>
      )}
    </span>
  )
}
