import { resolve } from 'node:path';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	resolve: {
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
