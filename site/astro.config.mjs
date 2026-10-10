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
      lastUpdated: true,
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'What is Natamux', slug: 'docs' },
            { slug: 'docs/getting-started/install' },
            { slug: 'docs/getting-started/quick-start' },
          ],
        },
        { label: 'Concepts', items: [{ autogenerate: { directory: 'docs/concepts' } }] },
        { label: 'Guides', items: [{ autogenerate: { directory: 'docs/guides' } }] },
        { label: 'Agents', items: [{ autogenerate: { directory: 'docs/agents' } }] },
        { label: 'Reference', items: [{ autogenerate: { directory: 'docs/reference' } }] },
        { label: 'Help', items: [{ autogenerate: { directory: 'docs/help' } }] },
      ],
    }),
  ],
})
