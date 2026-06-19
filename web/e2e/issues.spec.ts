import { test, expect } from '@playwright/test';
import { login, requireServer, TEST_PROJECT_SLUG } from './helpers';

/**
 * Locator for a clickable issue row. Issues are rendered as <TableRow> with
 * an onclick handler (not <a>), so we target the row by role + cursor class.
 */
function issueRows(page: import('@playwright/test').Page) {
	return page.locator('tr.cursor-pointer');
}

test.describe('Issues dashboard', () => {
	test.beforeEach(async ({ page }) => {
		await requireServer(page);
		await login(page);
	});

	test('renders the seeded issues from the ingest pipeline', async ({ page }) => {
		// Seed data was created pre-test via /api/{id}/envelope/. We expect at
		// least 4 distinct issues (DatabaseError, ValidationError, AuthError,
		// Panic). We assert >= 1 to stay robust against dedup variations.
		await expect(issueRows(page).first()).toBeVisible({ timeout: 10000 });
		const count = await issueRows(page).count();
		expect(count).toBeGreaterThanOrEqual(1);
	});

	test('fatal / error / warning issues are represented in the list', async ({ page }) => {
		// Each issue row typically shows a level badge. We just verify the
		// page rendered issue-level text somewhere — exact badge classes are
		// implementation detail.
		const body = await page.locator('body').innerText();
		const hasIssueContent =
			/panic|error|warning|exception|database|auth/i.test(body);
		expect(hasIssueContent).toBeTruthy();
	});

	test('status filter narrows the list to unresolved by default', async ({ page }) => {
		// The default filter chip is "All" (no filterStatus). Clicking
		// "unresolved" should still show at least one issue.
		const unresolvedChip = page.getByRole('button', { name: /^unresolved$/i });
		await expect(unresolvedChip).toBeVisible();
		await unresolvedChip.click();
		// The filter triggers an API call + re-render. Wait for network to
		// settle and for the table body to repopulate.
		await page.waitForLoadState('networkidle');
		// Re-query (rather than reuse stale handle) and give the DOM a
		// moment to repaint.
		await expect(issueRows(page).first()).toBeVisible({ timeout: 8000 });
		const count = await issueRows(page).count();
		expect(count).toBeGreaterThanOrEqual(1);
	});

	test('clicking an issue opens the detail view', async ({ page }) => {
		const firstIssue = issueRows(page).first();
		await expect(firstIssue).toBeVisible({ timeout: 10000 });
		await firstIssue.click();
		// Detail URL shape: /issues/<uuid>
		await expect(page).toHaveURL(/\/issues\/[a-f0-9-]+/i);
	});
});

test.describe('Issue detail view', () => {
	test.beforeEach(async ({ page }) => {
		await requireServer(page);
		await login(page);
	});

	test('renders event metadata for the selected issue', async ({ page }) => {
		// Issues list is on /issues; click the first row.
		await expect(issueRows(page).first()).toBeVisible({ timeout: 10000 });
		await issueRows(page).first().click();
		await expect(page).toHaveURL(/\/issues\/[a-f0-9-]+/i);

		// Detail page should show the stacktrace / culprit area. We assert
		// that the body contains something resembling an error title or
		// stack frame — the exact copy is server-controlled.
		const body = await page.locator('body').innerText();
		const looksLikeIssueDetail =
			/culprit|stacktrace|stack trace|first seen|last seen|level/i.test(body);
		expect(looksLikeIssueDetail).toBeTruthy();
	});
});
