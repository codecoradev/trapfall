import { test, expect } from '@playwright/test';
import { login, requireServer } from './helpers';

test.describe('Mobile dashboard', () => {
	test.use({ viewport: { width: 390, height: 844 } });

	test.beforeEach(async ({ page }) => {
		await requireServer(page);
		await login(page);
	});

	test('mobile drawer navigation opens and routes between pages', async ({ page }) => {
		// The desktop nav is hidden; the hamburger opens the Sheet drawer.
		await expect(page.getByRole('link', { name: 'Settings' })).toBeHidden();
		await page.getByRole('button', { name: 'Open menu' }).click();
		await expect(page.getByRole('heading', { name: 'TrapFall' })).toBeVisible();

		await page.getByRole('link', { name: 'Projects' }).click();
		await expect(page).toHaveURL(/\/projects/);
		await expect(page.getByRole('heading', { name: 'Projects' })).toBeVisible();
	});

	test('issue detail stats stack on a narrow viewport', async ({ page }) => {
		await page.locator('tr.cursor-pointer').first().click();
		await expect(page).toHaveURL(/\/issues\/[a-f0-9-]+/i);
		await expect(page.getByText('First seen')).toBeVisible();
		await expect(page.getByText('Last seen')).toBeVisible();
		// No horizontal overflow at 390px.
		const overflow = await page.evaluate(
			() => document.documentElement.scrollWidth > document.documentElement.clientWidth
		);
		expect(overflow).toBe(false);
	});
});
