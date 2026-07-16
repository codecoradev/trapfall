/**
 * Timezone formatting logic — pure functions, no Svelte runes.
 *
 * Kept separate from the reactive store so it stays importable in plain
 * test files and non-component modules. The TrapFall server persists all
 * timestamps in UTC (immutable invariant); these helpers convert UTC ISO
 * strings into the configured display timezone.
 */

/**
 * Format a UTC ISO timestamp in the given IANA timezone.
 * Falls back to browser locale formatting on an invalid zone or input.
 */
export function formatInTimezone(iso: string, timezone: string): string {
	if (!iso) return '';
	const date = new Date(iso);
	// Guard against non-date input (e.g. malformed payloads) so callers never
	// see "Invalid Date" — return the raw string instead.
	if (Number.isNaN(date.getTime())) return iso;
	try {
		return new Intl.DateTimeFormat(undefined, {
			timeZone: timezone,
			dateStyle: 'short',
			timeStyle: 'medium'
		}).format(date);
	} catch {
		return date.toLocaleString();
	}
}

/**
 * Accessor injected by the reactive store so non-rune modules (e.g. utils.ts,
 * tests) can read the active display timezone without importing the runes
 * module. Defaults to the browser zone until the store wires it up.
 */
let tzAccessor: () => string = () => Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';

/** Internal — called once by the store to supply the live timezone. */
export function setTimezoneAccessor(fn: () => string): void {
	tzAccessor = fn;
}

/** The currently active display timezone (IANA name). */
export function activeTimezone(): string {
	return tzAccessor();
}
