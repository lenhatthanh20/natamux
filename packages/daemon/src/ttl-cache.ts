type Entry<T> = { value: Promise<T>; readAt?: number }

// Keeps the answer of a slow read for a short time, by key. The time counts from when the read
// ended, and callers that ask while a read runs share it. A failed read is forgotten at once, so
// the next caller tries again.
export class TtlCache<T> {
  private readonly entries = new Map<string, Entry<T>>()
  private readonly ttlMs: number
  private readonly now: () => number

  constructor(ttlMs: number, now: () => number = Date.now) {
    this.ttlMs = ttlMs
    this.now = now
  }

  get(key: string, read: () => Promise<T>): Promise<T> {
    const known = this.entries.get(key)
    if (
      known !== undefined &&
      (known.readAt === undefined || this.now() - known.readAt < this.ttlMs)
    ) {
      return known.value
    }
    const entry: Entry<T> = { value: read() }
    this.entries.set(key, entry)
    entry.value.then(
      () => {
        entry.readAt = this.now()
      },
      () => {
        if (this.entries.get(key) === entry) {
          this.entries.delete(key)
        }
      },
    )
    return entry.value
  }

  // A read that runs now is not shared with later callers either. Without a key: every key.
  forget(key?: string): void {
    if (key === undefined) {
      this.entries.clear()
    } else {
      this.entries.delete(key)
    }
  }
}
