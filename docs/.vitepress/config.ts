import { createConfig } from '@codecora/theme/vitepress/config'

export default createConfig({
  product: 'trapfall',
  title: 'TrapFall',
  description: 'Lightweight self-hosted error capture engine',
  accent: 'red',
  repo: 'trapfall',
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
})
