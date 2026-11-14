// The checks of the checkout form, as regular expressions.
export const patterns = {
  email: /^[\w.+-]+@[a-z\d-]+(\.[a-z\d-]+)*\.[a-z]{2,}$/i,
  coupon: /^(?:WELCOME|SPRING|VIP)\d{0,2}$/,
  zip: /^\d{5}(?:-\d{4})?$/,
  phone: /^\+?(\d{1,3})?[\s.-]?\(?\d{3}\)?[\s.-]?\d{3}[\s.-]?\d{4}$/,
  price: /^(?<dollars>\d+)(?:\.(?<cents>\d{2}))?$/u,
  slug: /^[a-z0-9]+(?:-[a-z0-9]+)*$/,
  spaces: /\s{2,}/g,
  notDigits: /[^0-9]/g,
  hex: /^#?([a-f\d]{3}|[a-f\d]{6})$/i,
  lookaround: /(?<=\$)\d+(?!\.\d)/,
  unicode: /\p{Letter}+é|\x41\cJ/v,
}

export const clean = (text) => text.replace(patterns.spaces, ' ').trim()
const matches = new RegExp('^order-(\\d+)$', 'm').test('order-42')
export default matches
