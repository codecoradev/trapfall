import { describe, it, expect, vi } from 'vitest';
import { cn, rowKeyActivate, crashRateBarClass, crashRateColor } from './utils';

describe('cn utility', () => {
	it('merges class names', () => {
		expect(cn('foo', 'bar')).toBe('foo bar');
	});

	it('handles conditional classes', () => {
		expect(cn('base', false && 'hidden', 'active')).toBe('base active');
	});

	it('deduplicates tailwind classes', () => {
		expect(cn('px-2', 'px-4')).toBe('px-4');
	});
});

function keyEvent(key: string): KeyboardEvent {
	return new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
}

describe('rowKeyActivate', () => {
	it('activates on Enter and prevents the default scroll', () => {
		const activate = vi.fn();
		const e = keyEvent('Enter');
		rowKeyActivate(e, activate);
		expect(activate).toHaveBeenCalledOnce();
		expect(e.defaultPrevented).toBe(true);
	});

	it('activates on Space', () => {
		const activate = vi.fn();
		rowKeyActivate(keyEvent(' '), activate);
		expect(activate).toHaveBeenCalledOnce();
	});

	it('ignores other keys', () => {
		const activate = vi.fn();
		rowKeyActivate(keyEvent('a'), activate);
		rowKeyActivate(keyEvent('Escape'), activate);
		expect(activate).not.toHaveBeenCalled();
	});
});

describe('crashRateColor / crashRateBarClass', () => {
	it('returns empty string for null rate', () => {
		expect(crashRateColor(null)).toBe('');
		expect(crashRateBarClass(null)).toBe('');
	});

	it('maps severity thresholds to semantic tokens', () => {
		expect(crashRateColor(0.5)).toBe('text-success');
		expect(crashRateColor(4.9)).toBe('text-warning');
		expect(crashRateColor(10)).toBe('text-destructive');

		expect(crashRateBarClass(0.5)).toBe('bg-success');
		expect(crashRateBarClass(4.9)).toBe('bg-warning');
		expect(crashRateBarClass(10)).toBe('bg-destructive');
	});
});
