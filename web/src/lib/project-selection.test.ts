import { beforeEach, describe, expect, it } from 'vitest';
import { getStoredProject, resolveProject, storeProject } from './project-selection';

const projects = [{ slug: 'alpha' }, { slug: 'beta' }];

describe('project selection', () => {
	beforeEach(() => localStorage.clear());

	it('defaults to the first project', () => {
		expect(resolveProject(projects, null)).toBe('alpha');
	});

	it('prefers a valid URL param and stores it', () => {
		expect(resolveProject(projects, 'beta')).toBe('beta');
		expect(getStoredProject()).toBe('beta');
	});

	it('restores the stored selection when the URL has none', () => {
		storeProject('beta');
		expect(resolveProject(projects, null)).toBe('beta');
	});

	it('ignores unknown slugs in URL and storage', () => {
		storeProject('gone');
		expect(resolveProject(projects, 'nope')).toBe('alpha');
	});
});
