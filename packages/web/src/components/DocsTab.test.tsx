import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { type ComponentProps, useState } from 'react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import type { StageFile } from '../lib/api'
import { useToasts } from '../lib/toasts'
import { DocsTab } from './DocsTab'

const SPEC = [
  '# Spec title',
  '',
  '## Scope',
  '',
  'Some **bold** words and a [site](https://example.com).',
  '',
  '- first',
  '- second',
  '',
  '1. one',
  '2. two',
  '',
  '| Name | Kind |',
  '| --- | --- |',
  '| spec.md | doc |',
  '',
  '```ts',
  'const answer = 42',
  '```',
  '',
  '- [x] done',
  '- [ ] todo',
  '',
].join('\n')

// Drafts are kept per task for the whole page, so each test uses its own task.
let taskCount = 0

type Files = Record<string, { text: string; modifiedAt: string }>

type Failures = { list?: string; read?: string; save?: string }

// The tab as ChangesView holds it: the picked file lives above it.
function Harness({
  first,
  ...props
}: Omit<ComponentProps<typeof DocsTab>, 'file' | 'onFileChange'> & { first: string | null }) {
  const [file, setFile] = useState(first)
  return <DocsTab {...props} file={file} onFileChange={setFile} />
}

// `fail` is read on each call, so a test can end a failure by changing it.
function docs(start: Files, fail: Failures = {}, first: string | null = null) {
  taskCount += 1
  const files: Files = { ...start }
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const loadFiles = vi.fn(async (): Promise<StageFile[]> => {
    if (fail.list !== undefined) {
      throw new Error(fail.list)
    }
    return Object.entries(files).map(([name, file]) => ({
      name,
      size: file.text.length,
      modifiedAt: file.modifiedAt,
    }))
  })
  const readFile = vi.fn(async (name: string) => {
    if (fail.read !== undefined) {
      throw new Error(fail.read)
    }
    return files[name]?.text ?? ''
  })
  const writeFile = vi.fn(async (name: string, text: string) => {
    if (fail.save !== undefined) {
      throw new Error(fail.save)
    }
    files[name] = { text, modifiedAt: '2026-10-11T09:59:00.000Z' }
  })
  const tree = () => (
    <QueryClientProvider client={client}>
      <Harness
        first={first}
        taskId={`T-${taskCount}`}
        loadFiles={loadFiles}
        readFile={readFile}
        writeFile={writeFile}
      />
    </QueryClientProvider>
  )
  const rendered = render(tree())
  const refresh = () => act(() => client.invalidateQueries({ queryKey: ['stage-files'] }))
  return {
    writeFile,
    refresh,
    // Another tab took the place of this one, then the user came back.
    remount: () => {
      rendered.unmount()
      render(tree())
    },
    // What an agent does: write the file again, then the next poll sees it.
    agentWrites: async (name: string, text: string, modifiedAt: string) => {
      files[name] = { text, modifiedAt }
      await refresh()
    },
    agentDeletes: async (name: string) => {
      delete files[name]
      await refresh()
    },
  }
}

const file = (text: string) => ({ text, modifiedAt: '2026-10-11T09:00:00.000Z' })
const view = () => screen.getByRole('region', { name: 'spec.md' })

beforeAll(async () => {
  // The parser is a lazy chunk. On a cold Windows runner that import can outlast findBy,
  // so the first test sees an empty file view.
  await import('./MarkdownBody')
})

beforeEach(() => {
  window.localStorage.clear()
  // Relative times read the clock, so the tests run at a set time.
  vi.setSystemTime(new Date('2026-10-11T10:00:00Z'))
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useToasts.setState({ toasts: [] })
  vi.useRealTimers()
})

describe('DocsTab', () => {
  it('shows a stage file as rendered markdown', async () => {
    docs({ 'spec.md': file(SPEC) })

    expect(await screen.findByRole('heading', { level: 1, name: 'Spec title' })).toBeTruthy()
    expect(screen.getByRole('heading', { level: 2, name: 'Scope' })).toBeTruthy()
    expect(screen.getByText('bold').tagName).toBe('STRONG')
    const lists = screen.getAllByRole('list')
    expect(lists.map((list) => list.tagName)).toEqual(['UL', 'OL', 'UL'])
    const table = screen.getByRole('table')
    expect(within(table).getByRole('columnheader', { name: 'Name' })).toBeTruthy()
    expect(within(table).getByRole('cell', { name: 'spec.md' })).toBeTruthy()
    expect(screen.getByText('const answer = 42').closest('pre')).toBeTruthy()
    expect(within(view()).queryByText(/^#/)).toBeNull()
    expect(within(view()).queryByText(/\|/)).toBeNull()
  })

  it('opens a web link outside the app', async () => {
    docs({ 'spec.md': file(SPEC) })

    const link = await screen.findByRole('link', { name: 'site' })
    expect(link.getAttribute('href')).toBe('https://example.com')
    expect(link.getAttribute('target')).toBe('_blank')
    expect(link.getAttribute('rel')).toContain('noreferrer')
  })

  it('shows read-only task list boxes', async () => {
    docs({ 'spec.md': file(SPEC) })
    await screen.findByRole('heading', { level: 1 })

    const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[]
    expect(boxes.map((box) => box.checked)).toEqual([true, false])
    await userEvent.setup().click(boxes[1] as HTMLInputElement)
    expect((boxes[1] as HTMLInputElement).checked).toBe(false)
  })

  it('never runs or inserts the HTML of a file', async () => {
    const alert = vi.spyOn(window, 'alert').mockImplementation(() => {})
    docs({
      'spec.md': file(
        [
          '# Title',
          '',
          '<script>alert(1)</script>',
          '',
          '<img src=x onerror=alert(1)>',
          '',
          '![logo](https://example.com/logo.png)',
          '',
          '[run](javascript:alert(1))',
          '',
        ].join('\n'),
      ),
    })
    await screen.findByRole('heading', { level: 1, name: 'Title' })

    const region = view()
    expect(region.querySelector('script')).toBeNull()
    expect(region.querySelector('img')).toBeNull()
    expect(region.querySelector('[onerror]')).toBeNull()
    expect(within(region).queryByRole('link', { name: 'run' })).toBeNull()
    expect(within(region).getByText('run')).toBeTruthy()
    expect(within(region).getByText('logo')).toBeTruthy()
    expect(alert).not.toHaveBeenCalled()
  })

  it('shows the raw text in Source mode and keeps the choice', async () => {
    const user = userEvent.setup()
    docs({ 'spec.md': file(SPEC) })
    await screen.findByRole('heading', { level: 1 })

    const mode = screen.getByRole('radiogroup', { name: 'Show the file' })
    expect(within(mode).getByRole('radio', { name: 'Rendered' }).getAttribute('aria-checked')).toBe(
      'true',
    )
    await user.click(within(mode).getByRole('radio', { name: 'Source' }))

    expect(screen.queryByRole('heading', { level: 1 })).toBeNull()
    expect(view().querySelector('pre')?.textContent).toBe(SPEC)
    expect(window.localStorage.getItem('natamux.docs-source')).toBe('true')

    // A new visit starts in the mode last picked.
    cleanup()
    docs({ 'spec.md': file(SPEC) })
    await screen.findByText(/^# Spec title/)
    expect(screen.getByRole('radio', { name: 'Source' }).getAttribute('aria-checked')).toBe('true')
  })

  it('edits the raw text and shows the saved text rendered', async () => {
    const user = userEvent.setup()
    const { writeFile } = docs({ 'spec.md': file('# Old title\n') })
    await screen.findByRole('heading', { level: 1, name: 'Old title' })

    await user.click(screen.getByRole('button', { name: 'Edit' }))
    const box = screen.getByRole('textbox', { name: 'Text of spec.md' }) as HTMLTextAreaElement
    expect(box.value).toBe('# Old title\n')
    expect(screen.queryByRole('radiogroup', { name: 'Show the file' })).toBeNull()

    await user.clear(box)
    await user.type(box, '# New title')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(writeFile).toHaveBeenCalledWith('spec.md', '# New title')
    expect(await screen.findByRole('heading', { level: 1, name: 'New title' })).toBeTruthy()
    expect(useToasts.getState().toasts.map((item) => item.text)).toContain('Saved spec.md.')
    expect(screen.getByRole('radiogroup', { name: 'Show the file' })).toBeTruthy()
  })

  it('asks before it drops an edit on Cancel', async () => {
    const user = userEvent.setup()
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true)
    docs({ 'spec.md': file('# Title\n') })
    await screen.findByRole('heading', { level: 1, name: 'Title' })

    await user.click(screen.getByRole('button', { name: 'Edit' }))
    await user.type(screen.getByRole('textbox', { name: 'Text of spec.md' }), 'more')
    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    expect(confirm).toHaveBeenCalledWith('Drop your changes to spec.md?')
    expect(screen.getByRole('heading', { level: 1, name: 'Title' })).toBeTruthy()
  })

  it('shows the new text when an agent changes the open file', async () => {
    const { agentWrites } = docs({ 'spec.md': file('# First\n') })
    await screen.findByRole('heading', { level: 1, name: 'First' })

    await agentWrites('spec.md', '# Second\n', '2026-10-11T09:30:00.000Z')

    expect(await screen.findByRole('heading', { level: 1, name: 'Second' })).toBeTruthy()
  })
})

// A file that starts with "# Top", so a test can wait until it shows.
async function showRendered(lines: string[], fail: Failures = {}) {
  const result = docs({ 'spec.md': file(['# Top', '', ...lines].join('\n')) }, fail)
  await screen.findByRole('heading', { level: 1, name: 'Top' })
  return result
}

const pickMode = (name: 'Rendered' | 'Source') =>
  userEvent.setup().click(screen.getByRole('radio', { name }))

describe('DocsTab markdown', () => {
  it('renders the other inline and block elements', async () => {
    await showRendered([
      '*star* _under_ ~~gone~~ `**kept**`',
      '',
      '- outer',
      '  1. middle',
      '     - inner',
      '',
      '> quoted',
      '',
      '---',
      '',
      '#tag',
      '',
      '```',
      '# not a heading **nor bold**',
      '```',
      '',
      '| Left | Middle |',
      '| :--- | :---: |',
      '| a \\| b | c |',
    ])
    const region = view()

    expect(screen.getByText('star').tagName).toBe('EM')
    expect(screen.getByText('under').tagName).toBe('EM')
    expect(screen.getByText('gone').tagName).toBe('DEL')
    expect(screen.getByText('**kept**').tagName).toBe('CODE')
    const inner = screen.getByText('inner').closest('li')
    expect(inner?.parentElement?.closest('ol')?.parentElement?.closest('ul')).toBeTruthy()
    expect(screen.getByText('quoted').closest('blockquote')).toBeTruthy()
    expect(region.querySelector('hr')).toBeTruthy()
    expect(screen.getByText('#tag').tagName).toBe('P')
    expect(screen.getByText('# not a heading **nor bold**').closest('pre')).toBeTruthy()
    expect(screen.getAllByRole('heading')).toHaveLength(1)
    expect(screen.getByRole('cell', { name: 'a | b' })).toBeTruthy()
    expect(screen.getByRole('columnheader', { name: 'Middle' }).style.textAlign).toBe('center')
  })

  it('renders an empty file as an empty view', async () => {
    docs({ 'spec.md': file('') })
    await screen.findByRole('button', { name: 'Edit' })

    expect(view().querySelector('.hm-markdown')?.textContent).toBe('')
  })

  it('renders a large file', async () => {
    const lines = Array.from({ length: 1250 }, (_, index) => [
      `## Part ${index + 1}`,
      '',
      `Text of part ${index + 1}.`,
      '',
    ]).flat()
    docs({ 'spec.md': file(lines.join('\n')) })

    expect(await screen.findByText('Text of part 1250.')).toBeTruthy()
    expect(screen.getAllByRole('heading', { level: 2 })).toHaveLength(1250)
  })

  it('renders CRLF line ends like LF ones', async () => {
    docs({ 'spec.md': file('# Top\r\n\r\n- one\r\n- two\r\n') })

    expect(await screen.findByRole('heading', { level: 1, name: 'Top' })).toBeTruthy()
    expect(screen.getAllByRole('listitem').map((item) => item.textContent)).toEqual(['one', 'two'])
  })

  it('renders text that is not ASCII', async () => {
    await showRendered(['Tiếng Việt có dấu, 日本語のテキスト, 🚀 emoji'])

    expect(screen.getByText('Tiếng Việt có dấu, 日本語のテキスト, 🚀 emoji')).toBeTruthy()
  })

  it('shows code blocks as plain code, without the language name', async () => {
    await showRendered([
      '```ts',
      'const a = 1',
      '```',
      '',
      '    indented code',
      '',
      '```',
      '```',
      '',
      '```mermaid',
      'graph TD; A-->B',
      '```',
      '',
      '```',
      'never closed',
    ])
    const blocks = [...view().querySelectorAll('pre')].map((block) => block.textContent)

    expect(blocks).toEqual([
      'const a = 1\n',
      'indented code\n',
      '',
      'graph TD; A-->B\n',
      'never closed\n',
    ])
    expect(view().querySelector('pre span')).toBeNull()
  })

  it('shows math, footnotes and front matter as plain content', async () => {
    docs({
      'spec.md': file(
        [
          '---',
          'title: x',
          '---',
          '',
          '# Top',
          '',
          'Area is $x^2$.',
          '',
          'See[^1].',
          '',
          '[^1]: A note.',
        ].join('\n'),
      ),
    })
    await screen.findByRole('heading', { level: 1, name: 'Top' })

    expect(screen.getByText('Area is $x^2$.')).toBeTruthy()
    expect(screen.getByText('A note.', { exact: false })).toBeTruthy()
    expect(screen.getByText('title: x')).toBeTruthy()
  })
})

describe('DocsTab task lists', () => {
  it('keeps every kind of task box read only', async () => {
    const user = userEvent.setup()
    const { writeFile } = await showRendered([
      '- [X] upper',
      '- [ ] parent',
      '  - [x] child',
      '',
      '1. [ ] numbered',
      '',
      'A paragraph with [ ] in it.',
    ])
    const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[]

    expect(boxes.map((box) => box.checked)).toEqual([true, false, true, false])
    expect(screen.getByText('numbered').closest('ol')).toBeTruthy()
    expect(screen.getByText('A paragraph with [ ] in it.').tagName).toBe('P')
    for (const box of boxes) {
      expect(box.disabled).toBe(true)
      await user.click(box)
      await user.type(box, ' ')
    }
    expect(boxes.map((box) => box.checked)).toEqual([true, false, true, false])
    expect(writeFile).not.toHaveBeenCalled()
  })
})

describe('DocsTab safety', () => {
  it('drops HTML wherever it sits, and keeps it as text in code', async () => {
    const alert = vi.spyOn(window, 'alert').mockImplementation(() => {})
    await showRendered([
      '<iframe src="https://example.com"></iframe>',
      '',
      '<style>body { display: none }</style>',
      '',
      '<a href="javascript:alert(1)">raw link</a>',
      '',
      '<div onclick="alert(1)">click</div>',
      '',
      '<svg onload="alert(1)"></svg>',
      '',
      '| Cell |',
      '| --- |',
      '| <img src=x onerror=alert(1)> |',
      '',
      '- item <b onmouseover=alert(1)>bold</b>',
      '',
      '> quote <script>alert(1)</script>',
      '',
      '```html',
      '<script>alert(1)</script>',
      '```',
      '',
      '![local](./a.png)',
    ])
    const region = view()

    expect(
      region.querySelector('iframe, style, script, img, svg, b, [onclick], [onload], [onerror]'),
    ).toBeNull()
    expect(within(region).queryByRole('link')).toBeNull()
    expect(screen.getByText('<script>alert(1)</script>').closest('pre')).toBeTruthy()
    expect(screen.getByText('local')).toBeTruthy()
    expect(alert).not.toHaveBeenCalled()
  })
})

describe('DocsTab links', () => {
  it('opens web and mail links outside the app', async () => {
    await showRendered([
      '[secure](https://example.com) [plain](http://example.com) [mail](mailto:a@b.c)',
      '',
      'Auto <https://auto.example.com> and bare https://bare.example.com here.',
      '',
      'By [reference][1].',
      '',
      '[1]: https://ref.example.com',
    ])
    const links = within(view()).getAllByRole('link')

    expect(links.map((link) => link.getAttribute('href'))).toEqual([
      'https://example.com',
      'http://example.com',
      'mailto:a@b.c',
      'https://auto.example.com',
      'https://bare.example.com',
      'https://ref.example.com',
    ])
    for (const link of links) {
      expect(link.getAttribute('target')).toBe('_blank')
      expect(link.getAttribute('rel')).toBe('noopener noreferrer')
    }
  })

  it('shows any other link target as plain text', async () => {
    const alert = vi.spyOn(window, 'alert').mockImplementation(() => {})
    await showRendered([
      '[a](javascript:alert(1)) [b](JavaScript:alert(1)) [c]( javascript:alert(1))',
      '',
      '[d](java&#x09;script:alert(1)) [e](data:text/html,hi) [f](vbscript:msgbox)',
      '',
      '[g](file:///etc/passwd) [h](plan.md) [i](#sec)',
    ])

    expect(within(view()).queryByRole('link')).toBeNull()
    for (const name of ['a', 'b', 'c', 'e', 'f', 'g', 'h', 'i']) {
      const text = screen.getByText(name)
      expect(text.tagName).toBe('SPAN')
      await userEvent.setup().click(text)
    }
    expect(alert).not.toHaveBeenCalled()
  })
})

describe('DocsTab view mode', () => {
  it('stores Rendered again when the user picks it', async () => {
    await showRendered([])
    await pickMode('Source')
    await pickMode('Rendered')

    expect(screen.getByRole('heading', { level: 1, name: 'Top' })).toBeTruthy()
    expect(window.localStorage.getItem('natamux.docs-source')).toBe('false')
    expect(window.localStorage.getItem('natamux.changes-view')).toBeNull()
  })

  it('starts in Rendered mode when the stored value is not one it knows', async () => {
    window.localStorage.setItem('natamux.docs-source', 'foo')
    await showRendered([])

    expect(screen.getByRole('radio', { name: 'Rendered' }).getAttribute('aria-checked')).toBe(
      'true',
    )
  })

  it('works when the browser storage throws', async () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked')
    })
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked')
    })
    await showRendered([])
    await pickMode('Source')

    expect(view().querySelector('pre')?.textContent).toBe('# Top\n')
  })

  it('keeps the mode when another file is picked', async () => {
    const user = userEvent.setup()
    docs({ 'plan.md': file('# Plan\n'), 'spec.md': file('# Spec\n') }, {}, 'spec.md')
    await screen.findByRole('heading', { level: 1, name: 'Spec' })
    await pickMode('Source')
    await user.click(screen.getByRole('button', { name: /^plan\.md/ }))

    const region = await screen.findByRole('region', { name: 'plan.md' })
    expect(await within(region).findByText('# Plan')).toBeTruthy()
    expect(region.querySelector('pre')).toBeTruthy()
  })

  it('keeps spaces, tabs and blank lines in Source mode', async () => {
    const raw = '# Top  \n\n\n\tTabbed line\t\ntrailing   \n\n'
    docs({ 'spec.md': file(raw) })
    await screen.findByRole('heading', { level: 1, name: 'Top' })
    await pickMode('Source')

    expect(view().querySelector('pre')?.textContent).toBe(raw)
  })
})

describe('DocsTab editing', () => {
  it('keeps the draft when the user says no to dropping it', async () => {
    const user = userEvent.setup()
    vi.spyOn(window, 'confirm').mockReturnValue(false)
    await showRendered([])
    await user.click(screen.getByRole('button', { name: 'Edit' }))
    await user.type(screen.getByRole('textbox', { name: 'Text of spec.md' }), 'more')
    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    const box = screen.getByRole('textbox', { name: 'Text of spec.md' }) as HTMLTextAreaElement
    expect(box.value).toBe('# Top\nmore')
    expect(screen.getByText('edited')).toBeTruthy()
    expect(screen.queryByRole('radiogroup', { name: 'Show the file' })).toBeNull()
  })

  it('leaves an unchanged edit without asking', async () => {
    const user = userEvent.setup()
    const confirm = vi.spyOn(window, 'confirm')
    await showRendered([])
    await user.click(screen.getByRole('button', { name: 'Edit' }))
    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    expect(confirm).not.toHaveBeenCalled()
    expect(screen.getByRole('heading', { level: 1, name: 'Top' })).toBeTruthy()
  })

  it('goes back to Source mode after a save that began there', async () => {
    const user = userEvent.setup()
    await showRendered([])
    await pickMode('Source')
    await user.click(screen.getByRole('button', { name: 'Edit' }))
    const box = screen.getByRole('textbox', { name: 'Text of spec.md' })
    await user.clear(box)
    await user.type(box, '# Saved')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(await within(view()).findByText('# Saved')).toBeTruthy()
    expect(view().querySelector('pre')?.textContent).toBe('# Saved')
    expect(screen.getByRole('radio', { name: 'Source' }).getAttribute('aria-checked')).toBe('true')
  })

  it('stays in the edit with the toggle hidden when a save fails', async () => {
    const user = userEvent.setup()
    await showRendered([], { save: 'Disk full' })
    await user.click(screen.getByRole('button', { name: 'Edit' }))
    await user.type(screen.getByRole('textbox', { name: 'Text of spec.md' }), 'more')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() =>
      expect(useToasts.getState().toasts.map((item) => item.text)).toContain(
        'Could not save spec.md.',
      ),
    )
    expect(screen.getByRole('textbox', { name: 'Text of spec.md' })).toBeTruthy()
    expect(screen.queryByRole('radiogroup', { name: 'Show the file' })).toBeNull()
  })

  it('warns, and keeps the draft, when an agent changes the file during an edit', async () => {
    const user = userEvent.setup()
    const { agentWrites } = await showRendered([])
    await user.click(screen.getByRole('button', { name: 'Edit' }))
    await user.type(screen.getByRole('textbox', { name: 'Text of spec.md' }), 'mine')
    await agentWrites('spec.md', '# Theirs\n', '2026-10-11T09:30:00.000Z')

    expect(
      await screen.findByText(
        'This file changed after you began editing. Saving replaces the new text.',
      ),
    ).toBeTruthy()
    const box = screen.getByRole('textbox', { name: 'Text of spec.md' }) as HTMLTextAreaElement
    expect(box.value).toBe('# Top\nmine')
  })
})

describe('DocsTab live updates', () => {
  it('shows the new text in Source mode', async () => {
    const { agentWrites } = await showRendered([])
    await pickMode('Source')
    await agentWrites('spec.md', '# Next\n', '2026-10-11T09:30:00.000Z')

    await vi.waitFor(() => expect(view().querySelector('pre')?.textContent).toBe('# Next\n'))
  })

  it('keeps the shown elements when the same text is written again', async () => {
    const { agentWrites } = await showRendered(['Body text.'])
    const before = screen.getByText('Body text.')
    await agentWrites('spec.md', '# Top\n\nBody text.', '2026-10-11T09:30:00.000Z')

    await vi.waitFor(() => expect(screen.getByText('Body text.')).toBe(before))
  })

  it('shows the empty state when the open file is deleted', async () => {
    const { agentDeletes } = await showRendered([])
    await agentDeletes('spec.md')

    expect(await screen.findByText(/^No stage files yet/)).toBeTruthy()
  })
})

describe('DocsTab navigation', () => {
  it('renders the file that is picked, whatever its name', async () => {
    const user = userEvent.setup()
    docs({ 'notes.md': file('# Notes\n'), 'spec.md': file('# Spec\n') })
    await screen.findByRole('heading', { level: 1, name: 'Notes' })

    await user.click(screen.getByRole('button', { name: /^spec\.md/ }))
    await user.click(screen.getByRole('button', { name: /^notes\.md/ }))
    await user.click(screen.getByRole('button', { name: /^spec\.md/ }))

    const region = await screen.findByRole('region', { name: 'spec.md' })
    expect(await within(region).findByRole('heading', { level: 1, name: 'Spec' })).toBeTruthy()
    expect(within(region).queryByText('Notes')).toBeNull()
  })

  it('renders the file the Progress tab opens', async () => {
    docs({ 'plan.md': file('# Plan\n'), 'spec.md': file('# Spec\n') }, {}, 'plan.md')

    const region = await screen.findByRole('region', { name: 'plan.md' })
    expect(await within(region).findByRole('heading', { level: 1, name: 'Plan' })).toBeTruthy()
  })

  it('keeps the mode when the user comes back from another tab', async () => {
    const { remount } = await showRendered([])
    await pickMode('Source')
    remount()

    expect(await screen.findByText(/^# Top/)).toBeTruthy()
    expect(screen.getByRole('radio', { name: 'Source' }).getAttribute('aria-checked')).toBe('true')
  })
})

describe('DocsTab empty and error states', () => {
  it('shows the empty state without the mode toggle', async () => {
    docs({})

    expect(await screen.findByText(/^No stage files yet/)).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Write context.md' })).toBeTruthy()
    expect(screen.queryByRole('radiogroup')).toBeNull()
  })

  it('shows a list error, then the files once they load', async () => {
    const fail: Failures = { list: 'Daemon is down' }
    const { refresh } = docs({ 'spec.md': file('# Top\n') }, fail)

    expect((await screen.findByRole('alert')).textContent).toBe('Daemon is down')
    expect(screen.queryByRole('radiogroup')).toBeNull()

    fail.list = undefined
    await refresh()
    expect(await screen.findByRole('heading', { level: 1, name: 'Top' })).toBeTruthy()
  })

  it('shows a file error without the mode toggle', async () => {
    docs({ 'spec.md': file('# Top\n') }, { read: 'Cannot read spec.md' })

    expect((await screen.findByRole('alert')).textContent).toBe('Cannot read spec.md')
    expect(screen.getByRole('button', { name: 'Edit' })).toBeTruthy()
    expect(screen.queryByRole('radiogroup')).toBeNull()
  })
})

describe('DocsTab attachments', () => {
  const ATTACHMENTS = [
    { name: 'login-error.png', size: 12_300, type: 'image/png' },
    { name: 'spec.docx', size: 512, type: null },
  ]

  const original = { create: URL.createObjectURL, revoke: URL.revokeObjectURL }

  beforeEach(() => {
    URL.createObjectURL = vi.fn(() => 'blob:file')
    URL.revokeObjectURL = vi.fn()
  })

  afterEach(() => {
    URL.createObjectURL = original.create
    URL.revokeObjectURL = original.revoke
  })

  function withAttachments(stageFiles: Files) {
    taskCount += 1
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const readAttachment = vi.fn(async (name: string) => new Blob([name]))
    render(
      <QueryClientProvider client={client}>
        <Harness
          first={null}
          taskId={`T-${taskCount}`}
          loadFiles={async () =>
            Object.entries(stageFiles).map(([name, entry]) => ({
              name,
              size: entry.text.length,
              modifiedAt: entry.modifiedAt,
            }))
          }
          readFile={async (name) => stageFiles[name]?.text ?? ''}
          writeFile={async () => {}}
          loadAttachments={async () => ATTACHMENTS}
          readAttachment={readAttachment}
        />
      </QueryClientProvider>,
    )
    return { readAttachment }
  }

  it('lists the attachments under the stage files, with their sizes', async () => {
    withAttachments({ 'context.md': file('# Context') })

    const nav = await screen.findByRole('navigation', { name: 'Stage files' })
    expect(await within(nav).findByRole('heading', { name: 'Attachments' })).toBeTruthy()
    expect(within(nav).getByRole('button', { name: /login-error\.png/ }).textContent).toContain(
      '12.3 KB',
    )
    expect(within(nav).getByRole('button', { name: /spec\.docx/ }).textContent).toContain(
      '512 B · download',
    )
  })

  it('shows an attached image in place of the file until a file is picked', async () => {
    const user = userEvent.setup()
    const { readAttachment } = withAttachments({ 'context.md': file('# Context') })

    await user.click(await screen.findByRole('button', { name: /login-error\.png/ }))

    const region = await screen.findByRole('region', { name: 'login-error.png' })
    const image = await within(region).findByRole('img', { name: 'login-error.png' })
    expect(image.getAttribute('src')).toBe('blob:file')
    expect(readAttachment).toHaveBeenCalledWith('login-error.png')

    await user.click(screen.getByRole('button', { name: /context\.md/ }))
    expect(await screen.findByRole('region', { name: 'context.md' })).toBeTruthy()
  })

  it('downloads a file that is not an image', async () => {
    const user = userEvent.setup()
    const saved: string[] = []
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      saved.push(`${this.download} ${this.href}`)
    })
    const { readAttachment } = withAttachments({ 'context.md': file('# Context') })

    await user.click(await screen.findByRole('button', { name: /spec\.docx/ }))

    await vi.waitFor(() => expect(saved).toEqual(['spec.docx blob:file']))
    expect(readAttachment).toHaveBeenCalledWith('spec.docx')
    expect(screen.queryByRole('region', { name: 'spec.docx' })).toBeNull()
  })

  it('lists the attachments of a task with no stage file yet', async () => {
    withAttachments({})

    const nav = await screen.findByRole('navigation', { name: 'Attachments' })
    expect(within(nav).getByRole('button', { name: /login-error\.png/ })).toBeTruthy()
  })
})
