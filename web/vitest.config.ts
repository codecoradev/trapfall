import { resolve } from 'node:path';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [svelte({ hot: false })],
	resolve: {
		// Force Svelte's client bundles under Vitest; jsdom is not a server.
		conditions: ['browser'],
		alias: {
			$lib: resolve('./src/lib'),
			$components: resolve('./src/lib/components'),
			$ui: resolve('./src/lib/components/ui')
		}
	},
	test: {
		include: ['src/**/*.{test,spec}.{js,ts}'],
		environment: 'jsdom',
		globals: true,
		setupFiles: ['src/test-setup.ts']
	}
});
