# Storefront guide

The shop has a **catalog**, a *cart* and a `checkout`. This guide shows how they fit.

## Setup

1. Install the tools.
2. Start the shop: `make dev`.
3. Open <http://localhost:5173>.

> The first start builds the catalog, which takes a minute.

## Coupons

| Code    | Off  | Until      |
|---------|-----:|------------|
| WELCOME |  10% | always     |
| SPRING  |  25% | 2026-06-01 |

```ts
const amount = total(items) - Math.max(0, coupon)
```

- [x] Clamp the total at zero
- [ ] Show the coupon in the receipt

See [the coupon rules](https://example.com/coupons "Coupon rules") and ![logo](public/logo.png).

---

Last line with a footnote.[^1]

[^1]: Written for the storefront example.
