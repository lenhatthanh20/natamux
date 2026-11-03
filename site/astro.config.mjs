import starlight from '@astrojs/starlight'
import { defineConfig } from 'astro/config'

// The GitHub Pages site of the natamux organization: https://natamux.github.io/
export default defineConfig({
  site: 'https://natamux.github.io',
  trailingSlash: 'always',
  integrations: [
    starlight({
      title: 'Natamux',
      description:
        'Run many coding agents in parallel. Every task gets its own worktree, terminal and stages, on one dashboard.',
      logo: { src: './src/assets/icon.svg', alt: 'Natamux' },
      favicon: '/icon.svg',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/lenhatthanh20/natamux' },
      ],
      editLink: {
        baseUrl: 'https://github.com/lenhatthanh20/natamux/edit/main/site/',
      },
      customCss: ['./src/styles/docs.css'],
      components: {
        Header: './src/components/DocsHeader.astro',
        PageTitle: './src/components/DocsPageTitle.astro',
        PageFrame: './src/components/DocsPageFrame.astro',
        TableOfContents: './src/components/DocsTableOfContents.astro',
        MobileTableOfContents: './src/components/DocsMobileTableOfContents.astro',
      },
      lastUpdated: true,
      head: [
        { tag: 'meta', attrs: { property: 'og:image', content: 'https://natamux.github.io/media/native/og.jpg' } },
        { tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } },
        { tag: 'meta', attrs: { property: 'og:image:height', content: '630' } },
        { tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
        { tag: 'meta', attrs: { name: 'twitter:image', content: 'https://natamux.github.io/media/native/og.jpg' } },
      ],
      sidebar: [
        {
          label: 'Getting started', collapsed: true,
          items: [
            { label: 'Overview', slug: 'docs' },
            { slug: 'docs/getting-started/install' },
            { slug: 'docs/getting-started/quick-start' },
          ],
        },
        {
          label: 'Workflows', collapsed: true,
          items: [
            { slug: 'docs/guides/workflows' },
            { slug: 'docs/concepts/stages' },
            { slug: 'docs/concepts/review-loop' },
            { slug: 'docs/guides/mix-agents' },
            { slug: 'docs/guides/fan-out' },
            { slug: 'docs/guides/project-settings' },
            { slug: 'docs/concepts/agents-and-hooks' },
          ],
        },
        {
          label: 'Use the app', collapsed: true,
          items: [
            { slug: 'docs/guides/dashboard' },
            { slug: 'docs/concepts/tasks' },
            { slug: 'docs/concepts/workspaces' },
            { slug: 'docs/guides/terminal' },
            { slug: 'docs/guides/changes-and-history' },
            { slug: 'docs/guides/github' },
            { slug: 'docs/guides/command-line' },
            { slug: 'docs/guides/desktop-app' },
            { slug: 'docs/guides/status-icons' },
          ],
        },
        {
          label: 'Agents', collapsed: true,
          items: [
            { slug: 'docs/agents' },
            { slug: 'docs/agents/claude-code' },
            { slug: 'docs/agents/codex' },
            { slug: 'docs/agents/gemini' },
            { slug: 'docs/agents/copilot' },
            { slug: 'docs/agents/cursor' },
            { slug: 'docs/agents/other-agents' },
          ],
        },
        { label: 'Reference', collapsed: true, items: [{ autogenerate: { directory: 'docs/reference' } }] },
        {
          label: 'Help and hosting', collapsed: true,
          items: [
            { slug: 'docs/help/troubleshooting' },
            { slug: 'docs/help/faq' },
            { slug: 'docs/guides/self-host-docker' },
            { slug: 'docs/help/security-and-privacy' },
            { slug: 'docs/help/changelog' },
          ],
        },
      ],
    }),
  ],
})
