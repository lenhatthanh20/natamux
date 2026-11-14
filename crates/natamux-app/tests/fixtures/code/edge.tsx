import React, { useState, type ReactNode } from 'react'

type Props = { title: string; children?: ReactNode }

export function Panel({ title, children }: Props) {
  const [open, setOpen] = useState(false)
  const rest = { role: 'region', 'aria-label': title }
  return (
    <>
      {/* The header toggles the body. */}
      <header onClick={() => setOpen(!open)} data-open={open} {...rest}>
        <h2 className="title">{title}</h2>
        <svg:rect width={10} />
      </header>
      {open ? <section>{children}</section> : null}
      {open && <p>Open &amp; shown</p>}
    </>
  )
}

export const List = <T,>(props: { rows: T[] }) => <ul>{props.rows.map((row, i) => <li id={String(i)}>{String(row)}</li>)}</ul>
