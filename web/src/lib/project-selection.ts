/**
 * Persist the selected project slug across dashboard pages (issue #311).
 *
 * Resolution order: valid `?project=` URL param, then the last stored
 * selection, then the first available project.
 */
const STORAGE_KEY = 'trapfall.selectedProject';

export function getStoredProject(): string | null {
	try {
		return localStorage.getItem(STORAGE_KEY);
	} catch {
		return null;
	}
}

export function storeProject(slug: string): void {
	try {
		localStorage.setItem(STORAGE_KEY, slug);
	} catch {
		// Storage unavailable (private mode, blocked): selection just won't persist.
	}
}

export function resolveProject(
	projects: { slug: string }[],
	queryProject: string | null | undefined
): string {
	const valid = (s: string | null | undefined): s is string => !!s && projects.some((p) => p.slug === s);
	const stored = getStoredProject();
	const chosen = valid(queryProject) ? queryProject : valid(stored) ? stored : projects[0].slug;
	storeProject(chosen);
	return chosen;
}
