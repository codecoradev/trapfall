import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import EmptyState from './EmptyState.svelte';

describe('EmptyState', () => {
	it('renders title and description', () => {
		render(EmptyState, {
			props: { title: 'No issues found', description: 'Try adjusting your filters.' }
		});
		expect(screen.getByText('No issues found')).toBeInTheDocument();
		expect(screen.getByText('Try adjusting your filters.')).toBeInTheDocument();
	});

	it('omits the description when not provided', () => {
		render(EmptyState, { props: { title: 'Nothing here' } });
		expect(screen.getByText('Nothing here')).toBeInTheDocument();
		expect(screen.queryByText(/filters/i)).not.toBeInTheDocument();
	});
});
