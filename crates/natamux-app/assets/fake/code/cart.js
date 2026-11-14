// The cart of the shop, in plain JavaScript.
import { money } from './format.js'

const MAX_ITEMS = 50
export class Cart {
  #items = []

  add(item, count = 1) {
    if (this.#items.length + count > MAX_ITEMS) {
      throw new RangeError(`at most ${MAX_ITEMS} items`)
    }
    for (let i = 0; i < count; i++) this.#items.push({ ...item })
    return this
  }

  get total() {
    return this.#items.reduce((sum, { price }) => sum + price, 0)
  }

  async save(store = globalThis.localStorage) {
    const text = JSON.stringify(this.#items, null, 2)
    await Promise.resolve(store?.setItem('cart', text))
    return text.length > 0 ? 'saved' : null
  }
}

export const describe = (cart) => `${cart.total > 0 ? 'Total' : 'Empty'}: ${money(cart.total)}`
const isBig = (cart) => cart.total >= 1e3 && !Number.isNaN(cart.total)
export default { Cart, describe, isBig, version: 0x10, ready: true, missing: undefined }
