/**
 * A coupon rule.
 * @template T
 * @param {T} value - The value.
 * @returns {boolean}
 */
export function valid<T>(value: T): boolean {
  const code = /^[A-Z]{3,10}(?:-\d{2})?$/iu
  return typeof value === 'string' && code.test(value)
}

@sealed
class Ledger {
  #rows: bigint[] = [1_000n, 0x1fn]
  static readonly limit = 1e3
  private label?: string

  constructor(public readonly owner: string) {}

  *entries(): Generator<bigint> {
    yield* this.#rows
  }

  get size() {
    return this.#rows.length
  }
}

function sealed(target: Function): void {
  Object.seal(target)
}

const options = { round: true } satisfies Record<string, boolean>
const note = `total: ${options.round ? `${Math.round(1.5)}` : 'n/a'} é`
outer: for (const n of [1, 2]) {
  if (n > 1) break outer
}
export default Ledger?.name ?? null
type Pair<A, B = A> = [first: A, second?: B]
let maybe = undefined as unknown as keyof typeof options
