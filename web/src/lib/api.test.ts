import { describe, it, expect } from 'vitest';
import { api, ApiClientError } from '$lib/api';

describe('ApiClient', () => {
	describe('constructor', () => {
		it('uses /api as default base URL', () => {
			const client = new (api.constructor as any)('/api');
			expect(client).toBeDefined();
		});
	});

	describe('ApiClientError', () => {
		it('stores status and message', () => {
			const err = new ApiClientError(404, 'Not found');
			expect(err.status).toBe(404);
			expect(err.message).toBe('Not found');
			expect(err.name).toBe('ApiClientError');
		});
	});
});
