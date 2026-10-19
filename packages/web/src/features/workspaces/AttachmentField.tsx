import { fitAttachments, formatBytes } from '@natamux/core'
import { Paperclip, X } from 'lucide-react'
import { type ClipboardEvent, type DragEvent, useRef, useState } from 'react'
import { Button } from '../../components/ui/Button'
import { IconButton } from '../../components/ui/IconButton'
import { pastedName } from '../../lib/attachments'

// The id keys the row, as two files may share a name.
export type PickedFile = { id: number; file: File }

let lastId = 0

// A screenshot from the clipboard has no real name: "image.png" in Chrome, none elsewhere.
const CLIPBOARD_NAME = /^(image\.\w+)?$/

// The files of a new task, and the reasons for those that went over a limit.
export function useAttachments() {
  const [files, setFiles] = useState<PickedFile[]>([])
  const [refused, setRefused] = useState<string[]>([])
  const [dragging, setDragging] = useState(false)
  const pasted = useRef(0)

  const add = (added: readonly File[]) => {
    const result = fitAttachments(
      files.map(({ file }) => file),
      added,
    )
    const picked = result.kept.map((file) => {
      lastId += 1
      return { id: lastId, file }
    })
    setFiles([...files, ...picked])
    setRefused(result.refused.map((each) => each.reason))
  }

  const remove = (id: number) => {
    setFiles(files.filter((picked) => picked.id !== id))
    setRefused([])
  }

  // For the whole form, so a file dropped or pasted anywhere in the dialog is attached.
  const handlers = {
    onDragOver: (event: DragEvent) => {
      if (Array.from(event.dataTransfer.types).includes('Files')) {
        event.preventDefault()
        setDragging(true)
      }
    },
    onDragLeave: (event: DragEvent) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
        setDragging(false)
      }
    },
    onDrop: (event: DragEvent) => {
      setDragging(false)
      if (event.dataTransfer.files.length > 0) {
        event.preventDefault()
        add(Array.from(event.dataTransfer.files))
      }
    },
    onPaste: (event: ClipboardEvent) => {
      const items = Array.from(event.clipboardData.files)
      if (items.length === 0) {
        return
      }
      event.preventDefault()
      add(
        items.map((file) => {
          if (!CLIPBOARD_NAME.test(file.name)) {
            return file
          }
          pasted.current += 1
          return new File([file], pastedName(file.type, pasted.current), { type: file.type })
        }),
      )
    },
  }

  return { files, refused, dragging, add, remove, handlers }
}

type AttachmentFieldProps = {
  files: readonly PickedFile[]
  refused: readonly string[]
  dragging: boolean
  onAdd: (files: File[]) => void
  onRemove: (id: number) => void
}

export function AttachmentField({
  files,
  refused,
  dragging,
  onAdd,
  onRemove,
}: AttachmentFieldProps) {
  const picker = useRef<HTMLInputElement>(null)
  return (
    <>
      <div
        className={`flex flex-wrap items-center gap-2 rounded-md border border-dashed px-2.5 py-2 ${dragging ? 'border-focus bg-selected' : 'border-line-strong'}`}
      >
        <Button
          size="xs"
          variant="secondary"
          icon={Paperclip}
          onClick={() => picker.current?.click()}
        >
          Add files
        </Button>
        <span className="text-xs text-muted">
          Or drop or paste files here, such as screenshots, logs or specs. They are part of the
          context.
        </span>
        <input
          ref={picker}
          type="file"
          multiple
          hidden
          aria-label="Attach files"
          onChange={(event) => {
            onAdd(Array.from(event.target.files ?? []))
            // The same file can be picked again after it was taken out.
            event.target.value = ''
          }}
        />
      </div>
      {files.length > 0 && (
        <ul aria-label="Attached files" className="m-0 flex list-none flex-col gap-0.5 p-0">
          {files.map(({ id, file }) => (
            <li key={id} className="flex items-center gap-2 text-xs">
              <span className="min-w-0 flex-1 truncate font-mono text-ink">{file.name}</span>
              <span className="shrink-0 text-muted">{formatBytes(file.size)}</span>
              <IconButton
                label={`Remove ${file.name}`}
                icon={X}
                size="xs"
                onClick={() => onRemove(id)}
              />
            </li>
          ))}
        </ul>
      )}
      {refused.map((reason) => (
        <p key={reason} role="alert" className="m-0 text-xs text-remove">
          {reason}
        </p>
      ))}
    </>
  )
}
