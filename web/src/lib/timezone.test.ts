import { describe, it, expect } from 'vitest';
import { formatInTimezone } from './timezone';

describe('formatInTimezone', () => {
	const iso = '2026-07-01T03:39:29Z';

	it('renders in the given IANA timezone', () => {
		const jakarta = formatInTimezone(iso, 'Asia/Jakarta');
		const utc = formatInTimezone(iso, 'UTC');
		// Jakarta (UTC+7) and UTC (UTC+0) must differ for a 03:xxZ timestamp —
		// proving the timezone actually affects the output.
		expect(jakarta).not.toEqual(utc);
		expect(jakarta).toContain('10'); // 03 + 7h = 10
	});

	it('renders in UTC', () => {
		const out = formatInTimezone(iso, 'UTC');
		expect(out).toContain('39'); // minutes are timezone-stable
		expect(out).toMatch(/3|03/); // hour 3 (locale-dependent padding)
	});

	it('returns empty string for empty input', () => {
		expect(formatInTimezone('', 'UTC')).toBe('');
	});

	it('falls back gracefully for an invalid timezone', () => {
		// Should not throw; falls back to browser locale formatting.
		const out = formatInTimezone(iso, 'Not/A_Real_Zone');
		expect(out.length).toBeGreaterThan(0);
	});

	it('returns the raw input for an unparseable timestamp', () => {
		expect(formatInTimezone('not-a-date', 'UTC')).toBe('not-a-date');
	});
});
