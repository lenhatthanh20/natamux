import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import {
  fetchSignedSums,
  MAX_SIGNED_BYTES,
  RELEASE_SIGNER,
  verifySums,
} from './release-signature.js'
import { checkReleaseSignature, SIGNATURE_FIXTURE } from './smoke.js'

// Made by minisign 0.12 (Docker image jedisct1/minisign) with two throwaway keys, made with
// minisign -G -W -p <name>.pub -s <name>.sec. Releases are signed in the legacy form (-l).
//   SHA256SUMS.txt.legacy.minisig        -S -l -s test-signer.sec -t "natamux v1.2.3"
//   SHA256SUMS.txt.minisig               -S -s test-signer.sec -t "natamux v1.2.3" (prehashed)
//   SHA256SUMS.txt.other-legacy.minisig  -S -l -s other-signer.sec -t "natamux v1.2.3"
//   SHA256SUMS.txt.other.minisig         -S -s other-signer.sec -t "natamux v1.2.3" (prehashed)
function fixture(name: string): Buffer {
  return readFileSync(new URL(`./fixtures/minisign/${name}`, import.meta.url))
}

function publicKeyOf(name: string): string {
  return fixture(name).toString('utf8').split('\n')[1] ?? ''
}

const testSigner = publicKeyOf('test-signer.pub')
const otherSigner = publicKeyOf('other-signer.pub')
const sums = fixture('SHA256SUMS.txt')
const signed = fixture('SHA256SUMS.txt.legacy.minisig').toString('utf8')
const prehashed = fixture('SHA256SUMS.txt.minisig').toString('utf8')
const other = fixture('SHA256SUMS.txt.other-legacy.minisig').toString('utf8')
const otherPrehashed = fixture('SHA256SUMS.txt.other.minisig').toString('utf8')
const valid = { sums, signature: signed, publicKey: testSigner, tag: 'v1.2.3', current: '1.2.2' }

// Writes `bytes` into the signature line (alg 0-1, key id 2-9, signature 10-73) at `at`.
function patchSignature(signature: string, at: number, bytes: Buffer): string {
  const lines = signature.split('\n')
  const raw = Buffer.from(lines[1] ?? '', 'base64')
  bytes.copy(raw, at)
  lines[1] = raw.toString('base64')
  return lines.join('\n')
}

function keyIdOf(publicKey: string): Buffer {
  return Buffer.from(publicKey, 'base64').subarray(2, 10)
}

describe('verifySums', () => {
  it('accepts a legacy signature of minisign (-l)', () => {
    expect(() => verifySums(valid)).not.toThrow()
    expect(() => verifySums({ ...valid, signature: other, publicKey: otherSigner })).not.toThrow()
  })

  // Electron's crypto has no BLAKE2b-512, so the app could never check one.
  it('refuses a prehashed signature, the default of minisign', () => {
    expect(() => verifySums({ ...valid, signature: prehashed })).toThrow(
      'SHA256SUMS.txt.minisig is prehashed; releases are signed with minisign -S -l',
    )
    expect(() =>
      verifySums({ ...valid, signature: otherPrehashed, publicKey: otherSigner }),
    ).toThrow(/is prehashed/)
  })

  it('fails without a signature', () => {
    expect(() => verifySums({ ...valid, signature: '' })).toThrow(/is not a minisign signature/)
  })

  it('fails when SHA256SUMS.txt changed', () => {
    const changed = Buffer.from(sums.toString('utf8').replace('85158a9d', '85158a9e'))
    expect(() => verifySums({ ...valid, sums: changed })).toThrow(/signature of SHA256SUMS.txt/)
    const withCrlf = Buffer.from(sums.toString('utf8').replaceAll('\n', '\r\n'))
    expect(() => verifySums({ ...valid, sums: withCrlf })).toThrow(/signature of SHA256SUMS.txt/)
    const longer = Buffer.concat([sums, Buffer.from('\n')])
    expect(() => verifySums({ ...valid, sums: longer })).toThrow(/signature of SHA256SUMS.txt/)
  })

  it('fails when the trusted comment changed', () => {
    const signature = signed.replace(
      'trusted comment: natamux v1.2.3',
      'trusted comment: natamux v1.2.4',
    )
    expect(() => verifySums({ ...valid, signature, tag: 'v1.2.4' })).toThrow(
      /trusted comment in SHA256SUMS.txt.minisig is not signed/,
    )
  })

  it('fails when the comment names another release', () => {
    expect(() => verifySums({ ...valid, tag: 'v1.2.4' })).toThrow(
      'SHA256SUMS.txt.minisig is signed for "natamux v1.2.3", not for "natamux v1.2.4"',
    )
    expect(() => verifySums({ ...valid, tag: '1.2.3' })).toThrow(/not for "natamux 1.2.3"/)
  })

  it('fails when the release is not newer than the running version', () => {
    expect(() => verifySums({ ...valid, current: '1.2.3' })).toThrow(
      'v1.2.3 is not newer than 1.2.3',
    )
    expect(() => verifySums({ ...valid, current: '1.3.0' })).toThrow(/is not newer than 1.3.0/)
  })

  it('fails for another key, by its key id or by the signature', () => {
    expect(() => verifySums({ ...valid, signature: other })).toThrow(
      'SHA256SUMS.txt.minisig is signed by key 4823C61DA79D2A3F, not by the release key F8312C3014360C65',
    )
    const wrongId = patchSignature(signed, 2, Buffer.alloc(8, 1))
    expect(() => verifySums({ ...valid, signature: wrongId })).toThrow(/is signed by key/)
    // The other key's signature, labelled with the release key id.
    const relabelled = patchSignature(other, 2, keyIdOf(testSigner))
    expect(() => verifySums({ ...valid, signature: relabelled })).toThrow(
      /signature of SHA256SUMS.txt is not valid/,
    )
    expect(() => verifySums({ ...valid, publicKey: otherSigner })).toThrow(/is signed by key/)
  })

  it('fails for an unknown algorithm, even one minisign might add later', () => {
    const signature = patchSignature(signed, 0, Buffer.from('EX'))
    expect(() => verifySums({ ...valid, signature })).toThrow(/unknown algorithm/)
    // A prehashed signature passed off as legacy.
    const asLegacy = patchSignature(prehashed, 0, Buffer.from('Ed'))
    expect(() => verifySums({ ...valid, signature: asLegacy })).toThrow(
      /signature of SHA256SUMS.txt is not valid/,
    )
  })

  it('fails for a malformed signature file', () => {
    const lines = signed.split('\n')
    const withLines = (changed: string[]) => verifySums({ ...valid, signature: changed.join('\n') })
    expect(() => withLines(lines.slice(1))).toThrow(/not a minisign signature/)
    expect(() => withLines([...lines.slice(0, 4), 'extra', ''])).toThrow(/not a minisign signature/)
    expect(() => withLines([...lines.slice(0, 4), '', ''])).toThrow(/not a minisign signature/)
    expect(() => withLines(lines.map((line) => line.replace('untrusted comment: ', '')))).toThrow(
      /not a minisign signature/,
    )
    expect(() =>
      withLines(lines.map((line) => line.replace('trusted comment: ', 'comment: '))),
    ).toThrow(/not a minisign signature/)
    const line1 = lines[1] ?? ''
    const line3 = lines[3] ?? ''
    expect(() => withLines([lines[0] ?? '', `${line1}!`, ...lines.slice(2)])).toThrow(
      /signature in SHA256SUMS.txt.minisig is not valid base64 of 74 bytes/,
    )
    expect(() => withLines([lines[0] ?? '', line1.slice(0, -8), ...lines.slice(2)])).toThrow(
      /not valid base64 of 74 bytes/,
    )
    expect(() => withLines([...lines.slice(0, 3), line3.slice(0, -4), ''])).toThrow(
      /comment signature in SHA256SUMS.txt.minisig is not valid base64 of 64 bytes/,
    )
    expect(() => withLines([...lines.slice(0, 3), ` ${line3}`, ''])).toThrow(/not valid base64/)
  })

  it('fails for a malformed public key, such as a placeholder', () => {
    expect(() => verifySums({ ...valid, publicKey: 'PUT_THE_KEY_HERE' })).toThrow(
      /release public key is not valid base64 of 42 bytes/,
    )
    expect(() => verifySums({ ...valid, publicKey: '' })).toThrow(/release public key/)
    expect(() => verifySums({ ...valid, publicKey: testSigner.slice(0, -4) })).toThrow(
      /release public key/,
    )
    const notEd = Buffer.from(testSigner, 'base64')
    notEd.write('XX', 0, 'latin1')
    expect(() => verifySums({ ...valid, publicKey: notEd.toString('base64') })).toThrow(
      /not a minisign Ed25519 key/,
    )
  })

  // minisign drops a \r at the end of each line too, and the comment it signs has none.
  it('reads a signature file with CRLF line ends', () => {
    expect(() => verifySums({ ...valid, signature: signed.replaceAll('\n', '\r\n') })).not.toThrow()
    expect(() =>
      verifySums({ ...valid, signature: signed.replace('v1.2.3\n', 'v1.2.3\r\r\n') }),
    ).toThrow(/trusted comment in SHA256SUMS.txt.minisig is not signed/)
  })
})

describe('the signature check of the smoke', () => {
  it('holds the fixture files as they are', () => {
    expect(SIGNATURE_FIXTURE.publicKey).toBe(testSigner)
    expect(SIGNATURE_FIXTURE.sums).toBe(sums.toString('utf8'))
    expect(SIGNATURE_FIXTURE.signature).toBe(signed)
  })

  it('passes in Node.js', () => {
    expect(checkReleaseSignature()).toBe('the release signature check works')
  })
})

describe('fetchSignedSums', () => {
  const downloads = 'https://github.com/natamux/natamux/releases/download/v1.2.3/'
  const release = { tag: 'v1.2.3', version: '1.2.3', page: '', downloads }
  type Files = Record<string, (() => Response) | undefined>
  const served: Files = {
    'SHA256SUMS.txt': () => new Response(new Uint8Array(sums)),
    'SHA256SUMS.txt.minisig': () => new Response(signed),
  }

  function fetchFrom(files: Files): (url: string) => Promise<Response> {
    return async (url) => {
      const file = files[url.slice(downloads.length)]
      return file?.() ?? new Response('Not Found', { status: 404 })
    }
  }

  function signedSums(files: Files, current = '1.2.2') {
    return fetchSignedSums({ release, current, fetch: fetchFrom(files), publicKey: testSigner })
  }

  it('returns SHA256SUMS.txt of a signed release', async () => {
    expect((await signedSums(served)).equals(sums)).toBe(true)
  })

  it('checks the embedded release key by default', async () => {
    await expect(
      fetchSignedSums({ release, current: '1.2.2', fetch: fetchFrom(served) }),
    ).rejects.toThrow(/not by the release key 3F449A1E170BB7A0/)
  })

  it('fails without a signature, or when it cannot get one', async () => {
    const unsigned = { ...served, 'SHA256SUMS.txt.minisig': undefined }
    await expect(signedSums(unsigned)).rejects.toThrow(
      'Natamux 1.2.3 is not trusted: download of its signature SHA256SUMS.txt.minisig failed: HTTP 404',
    )
    const offline = {
      ...served,
      'SHA256SUMS.txt.minisig': () => {
        throw new Error('net::ERR_INTERNET_DISCONNECTED')
      },
    }
    await expect(signedSums(offline)).rejects.toThrow(/ERR_INTERNET_DISCONNECTED/)
    await expect(signedSums({ ...served, 'SHA256SUMS.txt': undefined })).rejects.toThrow(
      'Download of SHA256SUMS.txt failed: HTTP 404',
    )
  })

  it('fails for a bad signature, another release or an old version', async () => {
    const tampered = { ...served, 'SHA256SUMS.txt.minisig': () => new Response(prehashed) }
    await expect(signedSums(tampered)).rejects.toThrow(
      /^Natamux 1.2.3 is not trusted: its signature check failed. .* is prehashed/,
    )
    await expect(signedSums(served, '1.2.3')).rejects.toThrow(/v1.2.3 is not newer than 1.2.3/)
    const otherTag = { ...release, tag: 'v1.2.4', version: '1.2.4' }
    await expect(
      fetchSignedSums({
        release: otherTag,
        current: '1.2.2',
        fetch: fetchFrom(served),
        publicKey: testSigner,
      }),
    ).rejects.toThrow(/not for "natamux v1.2.4"/)
  })

  it('refuses an answer larger than 64 KB, by its header or by its body', async () => {
    const big = new Uint8Array(MAX_SIGNED_BYTES + 1)
    await expect(
      signedSums({ ...served, 'SHA256SUMS.txt': () => new Response(big) }),
    ).rejects.toThrow('SHA256SUMS.txt is larger than 65536 bytes')
    const lying = () => new Response(signed, { headers: { 'content-length': '70000' } })
    await expect(signedSums({ ...served, 'SHA256SUMS.txt.minisig': lying })).rejects.toThrow(
      /SHA256SUMS.txt.minisig is larger than 65536 bytes/,
    )
  })

  // The manual install kind (Windows and others) only opens the release page; it must not be
  // offered an unsigned release either.
  it('runs in the updater before the release page is offered or a file downloads', () => {
    const source = readFileSync(new URL('./updater.ts', import.meta.url), 'utf8')
    const check = source.indexOf('await fetchSignedSums(')
    expect(check).toBeGreaterThan(-1)
    expect(check).toBeLessThan(source.indexOf('await this.offerPage(release, manual)'))
    expect(check).toBeLessThan(source.indexOf('await this.download(release, name, sums)'))
  })
})

describe('RELEASE_SIGNER', () => {
  // CI's release-signer job also fails when a test key is embedded.
  it('is never the test key', () => {
    expect(RELEASE_SIGNER).not.toBe(testSigner)
    expect(RELEASE_SIGNER).not.toBe(otherSigner)
  })

  it('is a minisign key the check can read', () => {
    expect(() => verifySums({ ...valid, publicKey: RELEASE_SIGNER })).toThrow(
      'SHA256SUMS.txt.minisig is signed by key F8312C3014360C65, not by the release key 3F449A1E170BB7A0',
    )
  })
})
