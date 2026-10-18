import { describe, expect, it } from 'vitest'
import { pastedName, readAsUpload } from '../attachments'

describe('pastedName', () => {
  it('names a pasted image by its number and type', () => {
    expect(pastedName('image/png', 1)).toBe('pasted-1.png')
    expect(pastedName('image/jpeg', 2)).toBe('pasted-2.jpg')
    expect(pastedName('', 3)).toBe('pasted-3.png')
  })
})

describe('readAsUpload', () => {
  it('gives the name and the bytes as base64', async () => {
    const file = new File([new Uint8Array([0, 255, 104, 105])], 'a.bin')
    expect(await readAsUpload(file)).toEqual({ name: 'a.bin', data: 'AP9oaQ==' })
  })

  it('gives empty data for an empty file', async () => {
    expect(await readAsUpload(new File([], 'empty.txt'))).toEqual({ name: 'empty.txt', data: '' })
  })
})
