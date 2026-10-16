#!/usr/bin/env bash
# Makes the demo git repository /data/projects/storefront inside the demo daemon (once).
set -euo pipefail
docker exec -i "${DEMO_CONTAINER:-natamux-demo-daemon-1}" bash -s <<'EOF'
set -e
repo=/data/projects/storefront
[ -d "$repo/.git" ] && { echo "$repo exists"; exit 0; }
mkdir -p "$repo/src" "$repo/test"
cd "$repo"
git init -q -b main
git config user.name "Natamux demo"; git config user.email demo@example.invalid
printf '# Storefront\n\nA small shop: catalog, cart and checkout.\n' >README.md
printf '{\n  "name": "storefront",\n  "type": "module",\n  "scripts": { "test": "node --test" }\n}\n' >package.json
printf 'node_modules/\n' >.gitignore
git add -A && git commit -q -m "Start the storefront"
printf 'export type Item = { sku: string; price: number; qty: number }\n\nexport const total = (items: Item[]) =>\n  items.reduce((sum, item) => sum + item.price * item.qty, 0)\n' >src/cart.ts
printf "import { total, type Item } from './cart.js'\n\nexport function checkout(items: Item[], coupon = 0) {\n  return total(items) - coupon\n}\n" >src/checkout.ts
printf 'export const products = [\n  { sku: "mug", name: "Mug", price: 12 },\n  { sku: "tee", name: "T-shirt", price: 24 },\n]\n' >src/catalog.ts
printf "export const formatPrice = (value: number) => \`\$\${value.toFixed(2)}\`\n" >src/format.ts
printf "import { test } from 'node:test'\ntest('total of an empty cart', () => {})\n" >test/cart.test.ts
git add -A && git commit -q -m "Add the cart, checkout and catalog"
git commit -q --allow-empty -m "Set up CI"
echo "made $repo"
EOF
