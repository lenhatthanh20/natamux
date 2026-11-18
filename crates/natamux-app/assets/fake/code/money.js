/**
 * Money helpers of the shop.
 * @module money
 */

/**
 * The price of an item in cents.
 * @typedef {Object} Price
 * @property {number} cents - Whole cents, never negative.
 * @property {string} [currency='USD'] - An ISO 4217 code.
 */

/**
 * Formats a price for the cart.
 * @param {Price} price - The price to show.
 * @param {{ locale?: string }} [options] - How to show it.
 * @returns {string} The price with its currency sign.
 * @throws {RangeError} When the cents are negative.
 * @example
 * format({ cents: 1250 }) // '$12.50'
 * @see {@link https://example.com/money|the money rules}
 */
export function format(price, options = {}) {
  if (price.cents < 0) throw new RangeError('negative price')
  return new Intl.NumberFormat(options.locale ?? 'en-US', {
    style: 'currency',
    currency: price.currency ?? 'USD',
  }).format(price.cents / 100)
}

/** @deprecated Use {@link format}. */
export const show = format
