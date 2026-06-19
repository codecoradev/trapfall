import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright config for TrapFall E2E tests.
 *
 * The TrapFall server is expected to be running before these tests start.
 * Point `BASE_URL` at the server via env var, e.g.:
 *   BASE_URL=http://127.0.0.1:4601 npx playwright test
 *
 * Auth credentials are supplied via env vars to avoid hard-coding.
 */
export default defineConfig({
	testDir: './e2e',
	timeout: 30_000,
	expect: { timeout: 7_000 },
	// Run tests serially within a file — they share a single browser context
	// and step on each other if parallelised.
	workers: 1,
	retries: 0,
	reporter: [['list'], ['html', { open: 'never' }]],
	use: {
		baseURL: process.env.BASE_URL ?? 'http://127.0.0.1:4601',
		actionTimeout: 8_000,
		navigationTimeout: 12_000,
		trace: 'retain-on-failure',
		screenshot: 'only-on-failure',
		video: 'retain-on-failure',
		ignoreHTTPSErrors: true
	},
	projects: [
		{
			name: 'chromium',
			use: { ...devices['Desktop Chrome'] }
		}
	]
});
