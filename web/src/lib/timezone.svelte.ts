/**
 * Display-timezone store.
 *
 * The TrapFall server persists all timestamps in UTC (immutable invariant).
 * This store holds the configured **display** timezone fetched from
 * `/api/0/config`. The pure formatting logic lives in `timezone.ts` so it can
 * be used in non-component code and tests without pulling in runes.
 *
 * Defaults to the browser's local timezone until the server config loads, so
 * absolute timestamps are never wrong by more than a brief initial fetch.
 */
import { api } from './api';
import { formatInTimezone, setTimezoneAccessor } from './timezone';

let timezone = $state(Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC');

// Wire the live timezone into the plain-TS accessor so utils/tests can read
// it without importing this runes module.
setTimezoneAccessor(() => timezone);

let loaded = false;
let inflight: Promise<void> | null = null;

/**
 * Load the server's display timezone once per session. Safe to call
 * repeatedly; concurrent callers share a single in-flight request rather
 * than firing duplicate fetches.
 */
export async function loadTimezone(): Promise<void> {
	if (loaded) return;
	if (!inflight) {
		inflight = (async () => {
			try {
				const cfg = await api.getPublicConfig();
				if (cfg.timezone) timezone = cfg.timezone;
				loaded = true;
			} catch {
				// Keep the browser-default fallback. Not marked loaded so a later
				// call can retry (e.g. transient network failure during boot).
			} finally {
				inflight = null;
			}
		})();
	}
	return inflight;
}

/** Current display timezone (IANA name). */
export function getTimezone(): string {
	return timezone;
}

/** Format a UTC ISO timestamp in the configured display timezone. */
export function formatInTz(iso: string): string {
	return formatInTimezone(iso, timezone);
}
