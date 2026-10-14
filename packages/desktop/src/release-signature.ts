// Checks the minisign signature of SHA256SUMS.txt before the updater trusts any hash in it. Node
// crypto only, so it needs no Electron and no new package. See docs/design-desktop-update.md.
import { createPublicKey, type KeyObject, verify } from 'node:crypto'
import { isNewer, type Release, SIGNATURE_FILE, SUMS_FILE } from './update-check.js'

// The minisign public key of the release signer, key id 3F449A1E170BB7A0. The secret key never
// leaves the owner's Mac.
// scripts/sign-release.sh, scripts/release.test.sh and CI read this line.
export const RELEASE_SIGNER = 'RWSgtwsXHppEP5n4kgg1Q3yNIYODQNGsMQBKF93tjSRwzL1BrjeGoBpo'

type Signer = { id: Buffer; key: KeyObject }

const BASE64 = /^[A-Za-z0-9+/]+={0,2}$/
const UNTRUSTED = 'untrusted comment: '
const TRUSTED = 'trusted comment: '

function decode(text: string, length: number, what: string): Buffer {
  const bytes = Buffer.from(text, 'base64')
  if (!BASE64.test(text) || bytes.length !== length) {
    throw new Error(`${what} is not valid base64 of ${length} bytes`)
  }
  return bytes
}

// minisign shows a key id as a hex number; the file holds it little-endian.
function keyId(id: Buffer): string {
  return Buffer.from(id).reverse().toString('hex').toUpperCase()
}

// A public key as `minisign -P` takes it: base64 of "Ed", an 8-byte key id and a 32-byte key.
function readSigner(publicKey: string): Signer {
  const bytes = decode(publicKey, 42, 'The release public key')
  if (bytes.subarray(0, 2).toString('latin1') !== 'Ed') {
    throw new Error('The release public key is not a minisign Ed25519 key')
  }
  const x = bytes.subarray(10).toString('base64url')
  return {
    id: bytes.subarray(2, 10),
    key: createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x }, format: 'jwk' }),
  }
}

// Throws unless `signature`, the text of SHA256SUMS.txt.minisig, is a signature of `sums` by
// `publicKey` with the trusted comment "natamux <tag>", and the version of `tag` is newer than
// `current`. The comment ties the signed file to one release, so an old signed SHA256SUMS.txt
// cannot come back under a new tag.
export function verifySums(input: {
  sums: Uint8Array
  signature: string
  publicKey: string
  tag: string
  current: string
}): void {
  const { sums, signature, publicKey, tag, current } = input
  const signer = readSigner(publicKey)
  // minisign itself drops a \r at the end of each line, so a file saved with CRLF still verifies.
  const lines = signature.split('\n').map((line) => line.replace(/\r$/, ''))
  if (lines.at(-1) === '') {
    lines.pop()
  }
  const [untrusted, signatureLine, trustedLine, globalLine] = lines
  if (
    lines.length !== 4 ||
    !untrusted?.startsWith(UNTRUSTED) ||
    signatureLine === undefined ||
    !trustedLine?.startsWith(TRUSTED) ||
    globalLine === undefined
  ) {
    throw new Error(`${SIGNATURE_FILE} is not a minisign signature`)
  }

  // Only "Ed", the legacy form (minisign -S -l): Ed25519 over the file itself. The default "ED"
  // signs the BLAKE2b-512 of the file, and Electron's crypto (BoringSSL) has no BLAKE2b-512.
  const bytes = decode(signatureLine, 74, `The signature in ${SIGNATURE_FILE}`)
  const algorithm = bytes.subarray(0, 2).toString('latin1')
  if (algorithm === 'ED') {
    throw new Error(`${SIGNATURE_FILE} is prehashed; releases are signed with minisign -S -l`)
  }
  if (algorithm !== 'Ed') {
    throw new Error(`${SIGNATURE_FILE} uses an unknown algorithm`)
  }
  const id = bytes.subarray(2, 10)
  if (!id.equals(signer.id)) {
    throw new Error(
      `${SIGNATURE_FILE} is signed by key ${keyId(id)}, not by the release key ${keyId(signer.id)}`,
    )
  }
  const sig = bytes.subarray(10)
  if (!verify(null, sums, signer.key, sig)) {
    throw new Error(`The signature of ${SUMS_FILE} is not valid`)
  }

  const comment = trustedLine.slice(TRUSTED.length)
  const global = decode(globalLine, 64, `The comment signature in ${SIGNATURE_FILE}`)
  if (!verify(null, Buffer.concat([sig, Buffer.from(comment, 'utf8')]), signer.key, global)) {
    throw new Error(`The trusted comment in ${SIGNATURE_FILE} is not signed`)
  }
  if (comment !== `natamux ${tag}`) {
    throw new Error(`${SIGNATURE_FILE} is signed for "${comment}", not for "natamux ${tag}"`)
  }
  if (!isNewer(tag, current)) {
    throw new Error(`${tag} is not newer than ${current}`)
  }
}

// SHA256SUMS.txt is about 600 bytes and its signature about 300; a bigger answer is not ours.
export const MAX_SIGNED_BYTES = 64 * 1024

async function readSmall(response: Response, name: string): Promise<Buffer> {
  const tooBig = `${name} is larger than ${MAX_SIGNED_BYTES} bytes`
  if (Number(response.headers.get('content-length') ?? 0) > MAX_SIGNED_BYTES) {
    await response.body?.cancel()
    throw new Error(tooBig)
  }
  const reader = response.body?.getReader()
  const chunks: Uint8Array[] = []
  let size = 0
  for (;;) {
    const chunk = await reader?.read()
    if (chunk === undefined || chunk.done) {
      return Buffer.concat(chunks)
    }
    size += chunk.value.length
    if (size > MAX_SIGNED_BYTES) {
      await reader?.cancel()
      throw new Error(tooBig)
    }
    chunks.push(chunk.value)
  }
}

// Downloads SHA256SUMS.txt and its signature from `release` and returns SHA256SUMS.txt once
// verifySums passed. The updater calls it before it offers or downloads anything, so an unsigned
// release is never shown, not even as a link to its page.
export async function fetchSignedSums(input: {
  release: Release
  current: string
  fetch: (url: string) => Promise<Response>
  publicKey?: string
}): Promise<Buffer> {
  const { release, current, fetch, publicKey = RELEASE_SIGNER } = input
  const sumsResponse = await fetch(`${release.downloads}${SUMS_FILE}`)
  if (!sumsResponse.ok) {
    throw new Error(`Download of ${SUMS_FILE} failed: HTTP ${sumsResponse.status}`)
  }
  const sums = await readSmall(sumsResponse, SUMS_FILE)
  // No signature, no update: there is no fallback to the checksum alone.
  const signatureResponse = await fetch(`${release.downloads}${SIGNATURE_FILE}`)
  if (!signatureResponse.ok) {
    throw new Error(
      `Natamux ${release.version} is not trusted: download of its signature ${SIGNATURE_FILE} failed: HTTP ${signatureResponse.status}`,
    )
  }
  try {
    const signature = (await readSmall(signatureResponse, SIGNATURE_FILE)).toString('utf8')
    verifySums({ sums, signature, publicKey, tag: release.tag, current })
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error)
    throw new Error(
      `Natamux ${release.version} is not trusted: its signature check failed. ${reason}`,
    )
  }
  return sums
}
