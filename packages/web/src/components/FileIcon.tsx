import { DEFAULT_ICON, fileIconName } from '../lib/file-icons'
import { type ResolvedTheme, useResolvedTheme } from '../lib/theme'

// The bundled icons, by name. Keep the list in step with lib/file-icons.ts and lib/agents.ts
// (a test checks it).
const ICONS = import.meta.glob<string>(
  '/node_modules/material-icon-theme/icons/{agent,audio,babel,biome,bun,bun_light,c,certificate,changelog,claude,console,copilot,copilot_light,cpp,csharp,css,cursor,cursor_light,dart,database,diff,docker,document,editorconfig,elixir,eslint,file,font,gemfile,gemini-ai,git,go,go-mod,graphql,h,hpp,html,image,jar,java,javascript,json,jupyter,key,kotlin,less,license,lock,log,lua,makefile,markdown,mdx,nodejs,npm,pdf,php,playwright,pnpm,pnpm_light,powershell,prettier,proto,python,react,react_ts,readme,ruby,rust,sass,settings,svelte,svg,swift,table,terraform,test-js,test-jsx,test-ts,toml,toml_light,tsconfig,tune,typescript,typescript-def,vagrant,video,vite,vitest,vue,webassembly,xml,yaml,yarn,zip}.svg',
  { eager: true, query: '?url', import: 'default' },
)

// Icons the theme does not have.
const OWN_ICONS = import.meta.glob<string>('/src/icons/*.svg', {
  eager: true,
  query: '?url',
  import: 'default',
})

const urlOf = (name: string) =>
  ICONS[`/node_modules/material-icon-theme/icons/${name}.svg`] ??
  OWN_ICONS[`/src/icons/${name}.svg`]

// Some icons are dark on dark; the theme has a lighter one for them, made for light editors.
export function iconUrl(name: string, theme: ResolvedTheme): string | undefined {
  return (theme === 'light' ? urlOf(`${name}_light`) : undefined) ?? urlOf(name)
}

export function FileIcon({ path, className = '' }: { path: string; className?: string }) {
  const theme = useResolvedTheme()
  const url = iconUrl(fileIconName(path), theme) ?? iconUrl(DEFAULT_ICON, theme)
  return (
    <img
      src={url}
      alt=""
      aria-hidden="true"
      draggable={false}
      className={`size-4 shrink-0 select-none ${className}`}
    />
  )
}
