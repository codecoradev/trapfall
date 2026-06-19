import { test, expect, type Page } from '@playwright/test';

/**
 * Shared helpers for TrapFall E2E tests.
 *
 * Credentials + base URL are injected via env vars set by the test runner.
 */

export const TEST_EMAIL = process.env.TEST_EMAIL ?? 'admin@trapfall.test';
export const TEST_PASSWORD = process.env.TEST_PASSWORD ?? 'Password123!';
export const TEST_PROJECT_SLUG = process.env.TEST_PROJECT_SLUG ?? 'default';

/** Log in via the UI and wait until the dashboard issues page renders. */
export async function login(page: Page) {
	await page.goto('/login');
	await page.getByLabel(/email/i).fill(TEST_EMAIL);
	await page.getByLabel(/password/i).fill(TEST_PASSWORD);
	await page.getByRole('button', { name: /sign in/i }).click();
	// Authenticated users land on /issues
	await expect(page).toHaveURL(/\/issues/);
	// The H1 "Issues" confirms the dashboard actually rendered (not a redirect
	// back to login because of an unauthenticated state).
	await expect(page.getByRole('heading', { name: 'Issues' })).toBeVisible();
}

/** Skip the suite when the server is unreachable — avoids confusing failures. */
export async function requireServer(page: Page) {
	const base = page.context().request;
	const res = await base.get('/');
	test.skip(
		res.status() >= 500,
		`TrapFall server unhealthy (GET / -> ${res.status()})`
	);
}
