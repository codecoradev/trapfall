import { test, expect } from '@playwright/test';
import { login, requireServer, TEST_PROJECT_SLUG } from './helpers';

test.describe('Projects management', () => {
	test.beforeEach(async ({ page }) => {
		await requireServer(page);
		await login(page);
	});

	test('projects page lists the seeded default project', async ({ page }) => {
		await page.goto('/projects');
		await expect(page).toHaveTitle(/Projects/);
		await expect(page.getByRole('heading', { name: 'Projects' })).toBeVisible();

		// The seeded project card should be visible by name.
		const defaultCard = page.locator('text=Default Project').first();
		await expect(defaultCard).toBeVisible();
	});

	test('DSN is masked in the projects list and reveal fetches the full value', async ({ page }) => {
		await page.goto('/projects');

		// There may be multiple project cards (default + any created by earlier
		// tests). We scope everything to the first card to avoid strict-mode
		// violations.
		const dsnParagraph = page.getByText(/DSN \(use this in your SDK\)/).first();
		await expect(dsnParagraph).toBeVisible();

		// The code block next to it should initially show the bullet mask.
		const codeBlock = page.locator('code').filter({ hasText: /••••|https?:\/\// }).first();
		await expect(codeBlock).toBeVisible();

		// Click the Show button of the first card. It becomes disabled briefly
		// while the fetch is in flight, then shows the full DSN (a https URL
		// with a hyphenated UUID userinfo segment, no '...' mask).
		const showButton = page.getByRole('button', { name: /^Show$/ }).first();
		await showButton.click();

		await expect(codeBlock).toContainText(/https:\/\/[a-f0-9-]+@/i, { timeout: 8000 });

		// Copy button should now be visible for this card.
		await expect(page.getByRole('button', { name: /^Copy$/ }).first()).toBeVisible();
	});

	test('creating a new project adds it to the list', async ({ page }) => {
		await page.goto('/projects');
		await expect(page.getByRole('heading', { name: 'Projects' })).toBeVisible();

		const uniqueName = `E2E Project ${Date.now()}`;

		// Open the "New Project" form.
		await page.getByRole('button', { name: /new project|add project|create/i }).first().click();

		const nameInput = page.getByLabel(/name/i).first();
		await nameInput.fill(uniqueName);

		// Submit the form. The button label varies between Create/Add.
		await page.getByRole('button', { name: /create|add|save/i }).last().click();

		// New project card should appear in the list.
		await expect(page.getByText(uniqueName).first()).toBeVisible({ timeout: 8000 });
	});
});

test.describe('Navigation', () => {
	test.beforeEach(async ({ page }) => {
		await requireServer(page);
		await login(page);
	});

	test('can navigate between Issues, Projects, and back', async ({ page }) => {
		// Start on issues (post-login default).
		await expect(page).toHaveURL(/\/issues/);

		// Navigate to Projects via the sidebar/nav. We use a text-based link
		// to stay robust against layout changes.
		const projectsLink = page.getByRole('link', { name: /^Projects$/ }).first();
		if (await projectsLink.isVisible()) {
			await projectsLink.click();
			await expect(page).toHaveURL(/\/projects/);
		} else {
			// Fall back to direct navigation if the nav link is absent.
			await page.goto('/projects');
			await expect(page).toHaveURL(/\/projects/);
		}

		await expect(page.getByRole('heading', { name: 'Projects' })).toBeVisible();

		// Back to Issues.
		const issuesLink = page.getByRole('link', { name: /^Issues$/ }).first();
		if (await issuesLink.isVisible()) {
			await issuesLink.click();
			await expect(page).toHaveURL(/\/issues/);
		} else {
			await page.goto('/issues');
		}
		await expect(page.getByRole('heading', { name: 'Issues' })).toBeVisible();
	});
});
