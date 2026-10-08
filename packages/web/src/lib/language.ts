// The language that colors a file in the diff view, from its name. Anything unknown is plain text.
const BY_EXTENSION = new Map(
  Object.entries({
    ts: 'typescript',
    tsx: 'typescript',
    mts: 'typescript',
    cts: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    mjs: 'javascript',
    cjs: 'javascript',
    json: 'json',
    jsonc: 'json',
    css: 'css',
    scss: 'scss',
    less: 'less',
    html: 'html',
    htm: 'html',
    tpl: 'html',
    md: 'markdown',
    py: 'python',
    php: 'php',
    // Garoon's PHP files.
    csp: 'php',
    rb: 'ruby',
    go: 'go',
    rs: 'rust',
    java: 'java',
    kt: 'kotlin',
    swift: 'swift',
    c: 'c',
    h: 'c',
    cpp: 'cpp',
    cs: 'csharp',
    sql: 'sql',
    sh: 'shell',
    bash: 'shell',
    yml: 'yaml',
    yaml: 'yaml',
    xml: 'xml',
    svg: 'xml',
    toml: 'ini',
    ini: 'ini',
  }),
)

const BY_NAME = new Map([
  ['Dockerfile', 'dockerfile'],
  ['Makefile', 'makefile'],
])

export function languageForPath(path: string): string {
  const name = path.split('/').pop() ?? path
  const byName = BY_NAME.get(name)
  if (byName) {
    return byName
  }
  const dot = name.lastIndexOf('.')
  // A name that only starts with a dot (.gitignore) has no extension.
  if (dot <= 0) {
    return 'plaintext'
  }
  return BY_EXTENSION.get(name.slice(dot + 1).toLowerCase()) ?? 'plaintext'
}
