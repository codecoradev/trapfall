import { test, expect } from '@playwright/test';
import { login, requireServer } from './helpers';

test.describe('Auth flow', () => {
	test.beforeEach(async ({ page }) => {
		await requireServer(page);
	});

	test('login page renders with the expected form', async ({ page }) => {
		await page.goto('/login');
		await expect(page).toHaveTitle(/Login/);
		await expect(page.getByLabel(/email/i)).toBeVisible();
		await expect(page.getByLabel(/password/i)).toBeVisible();
		await expect(page.getByRole('button', { name: /sign in/i })).toBeVisible();
	});

	test('valid credentials land on the issues dashboard', async ({ page }) => {
		await login(page);
		await expect(page).toHaveURL(/\/issues/);
		await expect(page.getByRole('heading', { name: 'Issues' })).toBeVisible();
	});

	test('invalid credentials show an error and stay on /login', async ({ page }) => {
		await page.goto('/login');
		await page.getByLabel(/email/i).fill('admin@trapfall.test');
		await page.getByLabel(/password/i).fill('wrong-password');
		await page.getByRole('button', { name: /sign in/i }).click();

		// Should NOT navigate away from /login.
		await expect(page).toHaveURL(/\/login/);
		// An error message should appear (text is server-controlled; we only
		// assert that some destructive-coloured message becomes visible).
		await expect(page.locator('.text-destructive')).toBeVisible({ timeout: 4000 });
	});

	test('authenticated root (/) redirects to /issues', async ({ page }) => {
		await login(page);
		await page.goto('/');
		await expect(page).toHaveURL(/\/issues/);
	});

	test('logging out returns to /login', async ({ page }) => {
		await login(page);
		// Trigger logout via the API — the UI does this from a user menu, but
		// the menu's locator is brittle across layout changes. Hitting the
		// endpoint directly isolates the auth concern from the menu chrome.
		await page.request.post('/api/0/auth/logout');
		await page.goto('/');
		await expect(page).toHaveURL(/\/login/);
	});
});
