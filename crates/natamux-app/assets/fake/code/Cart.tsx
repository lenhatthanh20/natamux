// The cart panel of the shop.
import { useMemo, useState, type ReactNode } from 'react'
import { money } from './format'

type Item = { id: string; name: string; price: number }

interface CartProps {
  items: Item[]
  onCheckout?: (total: number) => void
  footer?: ReactNode
}

export function Cart({ items, onCheckout, footer }: CartProps) {
  const [coupon, setCoupon] = useState<number>(0)
  const total = useMemo(
    () => Math.max(0, items.reduce((sum, item) => sum + item.price, 0) - coupon),
    [items, coupon],
  )
  return (
    <section className="cart" aria-label="Cart">
      <ul>
        {items.map((item) => (
          <li data-id={item.id} data-price={item.price}>
            {item.name} <strong>{money(item.price)}</strong>
          </li>
        ))}
      </ul>
      <input type="number" value={coupon} onChange={(e) => setCoupon(Number(e.target.value))} />
      <button disabled={items.length === 0} onClick={() => onCheckout?.(total)}>
        Pay {money(total)}
      </button>
      {footer ?? <p>Free shipping from $50</p>}
    </section>
  )
}
