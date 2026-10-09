// Icons of file types, from Material Icon Theme (MIT), the icon set many VS Code users have. Only
// the icons named here are bundled; the theme's own manifest maps a thousand more names.

// Whole file names, lower case. They win over extensions: package.json is not just JSON.
const FILE_NAMES: Record<string, string> = {
  '.babelrc': 'babel',
  '.dockerignore': 'docker',
  '.editorconfig': 'editorconfig',
  '.env': 'tune',
  '.gitattributes': 'git',
  '.gitignore': 'git',
  '.gitmodules': 'git',
  '.npmrc': 'npm',
  '.nvmrc': 'nodejs',
  '.prettierrc': 'prettier',
  'agents.md': 'agent',
  'biome.json': 'biome',
  'biome.jsonc': 'biome',
  'bun.lock': 'bun',
  'bun.lockb': 'bun',
  'cargo.lock': 'lock',
  'cargo.toml': 'rust',
  'changelog.md': 'changelog',
  'claude.md': 'claude',
  'compose.yaml': 'docker',
  'compose.yml': 'docker',
  'composer.json': 'php',
  'docker-compose.yaml': 'docker',
  'docker-compose.yml': 'docker',
  dockerfile: 'docker',
  gemfile: 'gemfile',
  'go.mod': 'go-mod',
  'go.sum': 'go-mod',
  license: 'license',
  'license.md': 'license',
  'license.txt': 'license',
  makefile: 'makefile',
  'package-lock.json': 'npm',
  'package.json': 'nodejs',
  'playwright.config.ts': 'playwright',
  'pnpm-lock.yaml': 'pnpm',
  'pnpm-workspace.yaml': 'pnpm',
  'readme.md': 'readme',
  vagrantfile: 'vagrant',
  'yarn.lock': 'yarn',
}

// File names that start a certain way, such as tsconfig.build.json or vite.config.ts.
const NAME_STARTS: [string, string][] = [
  ['.env.', 'tune'],
  ['dockerfile.', 'docker'],
  ['eslint.config.', 'eslint'],
  ['tsconfig.', 'tsconfig'],
  ['vite.config.', 'vite'],
  ['vitest.config.', 'vitest'],
]

// Endings longer than one extension come first: a test is not plain TypeScript.
const ENDINGS: [string, string][] = [
  ['.d.ts', 'typescript-def'],
  ['.test.ts', 'test-ts'],
  ['.spec.ts', 'test-ts'],
  ['.test.tsx', 'test-jsx'],
  ['.spec.tsx', 'test-jsx'],
  ['.test.js', 'test-js'],
  ['.spec.js', 'test-js'],
  ['.test.mjs', 'test-js'],
  ['.spec.mjs', 'test-js'],
  ['.test.jsx', 'test-jsx'],
  ['.spec.jsx', 'test-jsx'],
]

const EXTENSIONS: Record<string, string> = {
  bash: 'console',
  bmp: 'image',
  c: 'c',
  cc: 'cpp',
  cfg: 'settings',
  cjs: 'javascript',
  conf: 'settings',
  cpp: 'cpp',
  crt: 'certificate',
  cs: 'csharp',
  css: 'css',
  csv: 'table',
  cts: 'typescript',
  cxx: 'cpp',
  dart: 'dart',
  diff: 'diff',
  dockerfile: 'docker',
  env: 'tune',
  ex: 'elixir',
  exs: 'elixir',
  gif: 'image',
  go: 'go',
  gql: 'graphql',
  graphql: 'graphql',
  gz: 'zip',
  h: 'h',
  hpp: 'hpp',
  htm: 'html',
  html: 'html',
  ico: 'image',
  ini: 'settings',
  ipynb: 'jupyter',
  jar: 'jar',
  java: 'java',
  jpeg: 'image',
  jpg: 'image',
  js: 'javascript',
  json: 'json',
  jsonc: 'json',
  jsx: 'react',
  key: 'key',
  kt: 'kotlin',
  less: 'less',
  lock: 'lock',
  log: 'log',
  lua: 'lua',
  md: 'markdown',
  mdx: 'mdx',
  mjs: 'javascript',
  mov: 'video',
  mp3: 'audio',
  mp4: 'video',
  mts: 'typescript',
  otf: 'font',
  patch: 'diff',
  pdf: 'pdf',
  pem: 'certificate',
  php: 'php',
  phar: 'php',
  png: 'image',
  proto: 'proto',
  ps1: 'powershell',
  py: 'python',
  rb: 'ruby',
  rs: 'rust',
  sass: 'sass',
  scss: 'sass',
  sh: 'console',
  sql: 'database',
  sqlite: 'database',
  svelte: 'svelte',
  svg: 'svg',
  swift: 'swift',
  tar: 'zip',
  tf: 'terraform',
  tgz: 'zip',
  toml: 'toml',
  ts: 'typescript',
  tsv: 'table',
  tsx: 'react_ts',
  ttf: 'font',
  txt: 'document',
  vue: 'vue',
  wasm: 'webassembly',
  wav: 'audio',
  webm: 'video',
  webp: 'image',
  woff: 'font',
  woff2: 'font',
  xml: 'xml',
  yaml: 'yaml',
  yml: 'yaml',
  zip: 'zip',
  zsh: 'console',
}

export const DEFAULT_ICON = 'file'

// The theme's name for the icon of a path.
export function fileIconName(path: string): string {
  const name = path.slice(path.lastIndexOf('/') + 1).toLowerCase()
  const byName = FILE_NAMES[name]
  if (byName !== undefined) {
    return byName
  }
  for (const [start, icon] of NAME_STARTS) {
    if (name.startsWith(start)) {
      return icon
    }
  }
  for (const [ending, icon] of ENDINGS) {
    if (name.endsWith(ending)) {
      return icon
    }
  }
  const dot = name.lastIndexOf('.')
  return (dot > 0 && EXTENSIONS[name.slice(dot + 1)]) || DEFAULT_ICON
}

// Every icon the maps above name, for a check that each one is bundled.
export function namedIcons(): string[] {
  return [
    ...new Set([
      DEFAULT_ICON,
      ...Object.values(FILE_NAMES),
      ...NAME_STARTS.map(([, icon]) => icon),
      ...ENDINGS.map(([, icon]) => icon),
      ...Object.values(EXTENSIONS),
    ]),
  ].sort()
}
