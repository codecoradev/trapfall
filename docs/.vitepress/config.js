import { defineConfig } from 'vitepress';

export default defineConfig({
  title: 'TrapFall',
  description: 'Lightweight self-hosted error capture engine',
  lang: 'en-US',
  cleanUrls: true,
  base: '/docs/trapfall/',
  head: [['link', { rel: 'icon', type: 'image/svg+xml', href: '/logo.svg' }]],
  themeConfig: {
    nav: [
      { text: 'Codecora', link: 'https://codecora.dev' },
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'Config', link: '/guide/configuration' },
      { text: 'API', link: '/guide/api' },
      { text: 'GitHub', link: 'https://github.com/codecoradev/trapfall' },
    ],
    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Getting Started', link: '/guide/getting-started' },
          { text: 'Configuration', link: '/guide/configuration' },
          { text: 'Multi-Project', link: '/guide/multi-project' },
          { text: 'SDK Integration', link: '/guide/sdk-integration' },
          { text: 'Docker', link: '/guide/docker' },
          { text: 'VPS Deployment', link: '/guide/vps-deployment' },
          { text: 'CLI Reference', link: '/guide/cli' },
          { text: 'SQLite → Postgres Migration', link: '/guide/migration' },
          { text: 'API Reference', link: '/guide/api' },
          { text: 'Alert Rules', link: '/guide/alerts' },
          { text: 'Search', link: '/guide/search' },
          { text: 'Security', link: '/guide/security' },
          { text: 'MCP Server', link: '/guide/mcp' },
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
