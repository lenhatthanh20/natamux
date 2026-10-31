// Orders of the shop: states, totals and a small store.
import { total, type Item } from './cart.js'

export type OrderState = 'open' | 'paid' | 'sent' | 'cancelled'

export interface Order {
  readonly id: number
  items: Item[]
  state: OrderState
  coupon?: number
}

export enum Channel {
  Web = 'web',
  Shop = 'shop',
}

const TAX = 0.0825 as const

export abstract class Store<T extends { id: number }> {
  protected rows = new Map<number, T>()
  abstract validate(row: T): row is T
  put(row: T): this {
    if (!this.validate(row)) throw new TypeError(`bad row ${row.id}`)
    this.rows.set(row.id, row)
    return this
  }
}

export function orderTotal(order: Order): number {
  const amount = total(order.items) - Math.max(0, order.coupon ?? 0)
  return Number((Math.max(0, amount) * (1 + TAX)).toFixed(2))
}

export const isOpen = <T extends Order>(order: T): boolean => order.state === 'open'
declare global {
  interface Window { shopVersion: string }
}
