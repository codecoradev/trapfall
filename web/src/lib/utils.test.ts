import { describe, it, expect } from 'vitest';
import { cn } from '$lib/utils';

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
