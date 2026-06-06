/**
 * TrapFall API client — typed wrapper around fetch for the TrapFall daemon API.
 */

const API_BASE = '/api';

export interface ApiError {
	error: string;
}

export interface UserInfo {
	id: string;
	email: string;
	name: string;
	role: string;
}

export interface Project {
	id: string;
	slug: string;
	name: string;
	dsn: string;
	created_at: string;
}

export interface SetupStatus {
	needs_setup: boolean;
}

export interface SetupResponse {
	user: UserInfo;
	project_slug: string;
	dsn: string;
}

export interface LoginResponse {
	user: UserInfo;
}

class ApiClient {
	private baseUrl: string;

	constructor(baseUrl: string = API_BASE) {
		this.baseUrl = baseUrl;
	}

	private async request<T>(
		path: string,
		options: RequestInit = {}
	): Promise<T> {
		const url = `${this.baseUrl}${path}`;
		const res = await fetch(url, {
			...options,
			headers: {
				'Content-Type': 'application/json',
				...options.headers
			}
		});

		if (!res.ok) {
			const body = await res.json().catch(() => ({ error: res.statusText }));
			throw new ApiClientError(res.status, body.error || res.statusText);
		}

		return res.json();
	}

	async get<T>(path: string): Promise<T> {
		return this.request<T>(path, { method: 'GET' });
	}

	async post<T>(path: string, body?: unknown): Promise<T> {
		return this.request<T>(path, {
			method: 'POST',
			body: body ? JSON.stringify(body) : undefined
		});
	}

	async delete<T>(path: string): Promise<T> {
		return this.request<T>(path, { method: 'DELETE' });
	}

	// ── Auth ──────────────────────────────────────────────────────────

	async getSetupStatus(): Promise<SetupStatus> {
		return this.get<SetupStatus>('/setup');
	}

	async setup(email: string, name: string, password: string): Promise<SetupResponse> {
		return this.post<SetupResponse>('/setup', { email, name, password });
	}

	async login(email: string, password: string): Promise<LoginResponse> {
		return this.post<LoginResponse>('/auth/login', { email, password });
	}

	async logout(): Promise<void> {
		await this.post('/auth/logout');
	}

	async getMe(): Promise<UserInfo> {
		return this.get<UserInfo>('/auth/me');
	}

	// ── Projects ──────────────────────────────────────────────────────

	async listProjects(): Promise<Project[]> {
		return this.get<Project[]>('/0/projects');
	}

	async getProject(slug: string): Promise<Project> {
		return this.get<Project>(`/0/projects/${slug}`);
	}
}

export class ApiClientError extends Error {
	constructor(
		public status: number,
		message: string
	) {
		super(message);
		this.name = 'ApiClientError';
	}
}

export const api = new ApiClient();
