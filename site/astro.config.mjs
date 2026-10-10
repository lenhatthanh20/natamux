import starlight from '@astrojs/starlight'
import { defineConfig } from 'astro/config'

// A GitHub Pages project site: https://lenhatthanh20.github.io/natamux/
export default defineConfig({
  site: 'https://lenhatthanh20.github.io',
  base: '/natamux',
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
        { label: 'Concepts', autogenerate: { directory: 'docs/concepts' } },
        { label: 'Guides', autogenerate: { directory: 'docs/guides' } },
        { label: 'Agents', autogenerate: { directory: 'docs/agents' } },
        { label: 'Reference', autogenerate: { directory: 'docs/reference' } },
        { label: 'Help', autogenerate: { directory: 'docs/help' } },
      ],
    }),
  ],
})
