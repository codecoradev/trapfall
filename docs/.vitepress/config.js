import { defineConfig } from 'vitepress';

export default defineConfig({
  title: 'TrapFall',
  description: 'Lightweight self-hosted error capture engine',
  lang: 'en-US',
  base: '/docs/trapfall/',
  cleanUrls: true,
  head: [['link', { rel: 'icon', type: 'image/svg+xml', href: '/docs/trapfall/logo.svg' }]],
  themeConfig: {
    nav: [
      { text: 'Codecora', link: 'https://codecora.dev' },
      { text: 'Guide', link: '/docs/trapfall/guide/getting-started' },
      { text: 'Config', link: '/docs/trapfall/guide/configuration' },
      { text: 'API', link: '/docs/trapfall/guide/api' },
      { text: 'GitHub', link: 'https://github.com/codecoradev/trapfall' },
    ],
    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Getting Started', link: '/docs/trapfall/guide/getting-started' },
          { text: 'Configuration', link: '/docs/trapfall/guide/configuration' },
          { text: 'Multi-Project', link: '/docs/trapfall/guide/multi-project' },
          { text: 'SDK Integration', link: '/docs/trapfall/guide/sdk-integration' },
          { text: 'Docker', link: '/docs/trapfall/guide/docker' },
          { text: 'CLI Reference', link: '/docs/trapfall/guide/cli' },
          { text: 'API Reference', link: '/docs/trapfall/guide/api' },
          { text: 'Alert Rules', link: '/docs/trapfall/guide/alerts' },
          { text: 'Search', link: '/docs/trapfall/guide/search' },
          { text: 'Security', link: '/docs/trapfall/guide/security' },
          { text: 'MCP Server', link: '/docs/trapfall/guide/mcp' },
        ],
      },
    ],
    socialLinks: [
      { icon: 'github', link: 'https://github.com/codecoradev/trapfall' },
    ],
    search: {
      provider: 'local',
    },
  },
});
