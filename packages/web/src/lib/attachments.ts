import type { AttachmentUpload } from '@natamux/core'

const PASTED_EXTENSIONS: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/gif': 'gif',
  'image/webp': 'webp',
}

// A pasted screenshot comes as "image.png" or with no name at all, so each one gets its own.
export function pastedName(type: string, number: number): string {
  return `pasted-${number}.${PASTED_EXTENSIONS[type] ?? 'png'}`
}

// The daemon takes the files of a new task as base64 inside the JSON of the request.
export function readAsUpload(file: File): Promise<AttachmentUpload> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const url = String(reader.result)
      resolve({ name: file.name, data: url.slice(url.indexOf(',') + 1) })
    }
    reader.onerror = () => reject(new Error(`Could not read ${file.name}`))
    reader.readAsDataURL(file)
  })
}

// Saves the bytes under the name, as the browser saves a download.
export function saveBlob(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = name
  link.click()
  // The download has started by then.
  setTimeout(() => URL.revokeObjectURL(url), 60_000)
}
