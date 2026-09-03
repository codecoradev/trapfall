import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import Pagination from './Pagination.svelte';

describe('Pagination', () => {
	it('renders nothing when there is only one page', () => {
		const { container } = render(Pagination, {
			props: { page: 1, totalPages: 1, total: 12, perPage: 20, onPageChange: vi.fn() }
		});
		// Svelte leaves an empty comment node for the falsy branch; no real elements.
		expect(container.querySelector('*')).toBeNull();
		expect(screen.queryByText(/Showing/)).not.toBeInTheDocument();
	});

	it('shows the active range and page buttons', () => {
		render(Pagination, {
			props: { page: 2, totalPages: 3, total: 50, perPage: 20, onPageChange: vi.fn() }
		});
		expect(screen.getByText('Showing 21–40 of 50')).toBeInTheDocument();
		for (const n of [1, 2, 3]) {
			expect(screen.getByRole('button', { name: String(n) })).toBeInTheDocument();
		}
	});

	it('disables Prev on the first page and Next on the last', () => {
		const { unmount } = render(Pagination, {
			props: { page: 1, totalPages: 3, total: 50, perPage: 20, onPageChange: vi.fn() }
		});
		expect(screen.getByRole('button', { name: 'Prev' })).toBeDisabled();
		expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled();
		unmount();

		render(Pagination, {
			props: { page: 3, totalPages: 3, total: 50, perPage: 20, onPageChange: vi.fn() }
		});
		expect(screen.getByRole('button', { name: 'Prev' })).toBeEnabled();
		expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();
	});

	it('reports page changes through onPageChange', async () => {
		const onPageChange = vi.fn();
		render(Pagination, {
			props: { page: 2, totalPages: 3, total: 50, perPage: 20, onPageChange }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
		expect(onPageChange).toHaveBeenCalledWith(3);
		await fireEvent.click(screen.getByRole('button', { name: '1' }));
		expect(onPageChange).toHaveBeenCalledWith(1);
	});
});
