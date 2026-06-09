/**
 * Auth store — manages user session state for the TrapFall dashboard.
 */
import { browser } from '$app/environment';
import { api, type UserInfo } from '$lib/api';

interface AuthState {
	user: UserInfo | null;
	loading: boolean;
	initialized: boolean;
}

let state: AuthState = $state({
	user: null,
	loading: false,
	initialized: false
});

export function getAuthStore() {
	return {
		get user() { return state.user; },
		get loading() { return state.loading; },
		get initialized() { return state.initialized; },
		get isAuthenticated() { return !!state.user; },

		async init() {
			if (!browser || state.initialized) return;
			state.loading = true;
			try {
				state.user = await api.getMe();
			} catch {
				state.user = null;
			} finally {
				state.loading = false;
				state.initialized = true;
			}
		},

		async login(email: string, password: string) {
			state.loading = true;
			try {
				const res = await api.login(email, password);
				state.user = res.user;
				return res;
			} finally {
				state.loading = false;
			}
		},

		async logout() {
			try {
				await api.logout();
			} finally {
				state.user = null;
			}
		},

		async setup(email: string, name: string, password: string) {
			state.loading = true;
			try {
				const res = await api.setup(email, name, password);
				state.user = res.user;
				return res;
			} finally {
				state.loading = false;
			}
		}
	};
}
