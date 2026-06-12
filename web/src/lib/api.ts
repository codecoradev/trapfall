/**
 * TrapFall API client — typed wrapper around fetch for the TrapFall daemon API.
 */

const API_BASE = '/api/0';

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

export type IssueStatus = 'unresolved' | 'resolved' | 'ignored' | 'regression';
export type Level = 'fatal' | 'error' | 'warning' | 'info' | 'debug' | 'trace';

export interface Issue {
	id: string;
	project_id: string;
	fingerprint: string;
	title: string;
	culprit: string | null;
	status: IssueStatus;
	level: Level;
	count: number;
	user_count: number;
	first_seen: string;
	last_seen: string;
}

export interface StoredEvent {
	id: string;
	issue_id: string;
	project_id: string;
	data: Record<string, unknown>;
	received_at: string;
}

export interface ListResponse<T> {
	data: T[];
	total: number;
	page: number;
	per_page: number;
}

class ApiClient {
	private baseUrl: string;

	constructor(baseUrl: string = API_BASE) {
		this.baseUrl = baseUrl;
	}

	private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
		const url = `${this.baseUrl}${path}`;
		const res = await fetch(url, {
			...options,
			headers: {
				'Content-Type': 'application/json',
				...options.headers
			}
		});

		if (!res.ok) {
			// Redirect to login on 401
			if (res.status === 401 && !path.startsWith('/auth') && !path.startsWith('/setup')) {
				gotoLogin();
			}
			const body = await res.json().catch(() => ({ error: res.statusText }));
			throw new ApiClientError(res.status, body.error || res.statusText);
		}

		// Handle 200 with no body (e.g., logout, set_status)
		const text = await res.text();
		if (!text) return {} as T;
		return JSON.parse(text);
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
		return this.get<Project[]>('/projects');
	}

	async createProject(name: string, slug?: string): Promise<Project> {
		return this.post<Project>('/projects', { name, slug });
	}

	async getProject(slug: string): Promise<Project> {
		return this.get<Project>(`/projects/${slug}`);
	}

	// ── Issues ────────────────────────────────────────────────────────

	async listIssues(
		projectSlug: string,
		opts?: { page?: number; perPage?: number; status?: string; level?: string }
	): Promise<ListResponse<Issue>> {
		const page = opts?.page ?? 1;
		const perPage = opts?.perPage ?? 20;
		let path = `/projects/${projectSlug}/issues?page=${page}&per_page=${perPage}`;
		if (opts?.status) path += `&status=${opts.status}`;
		if (opts?.level) path += `&level=${opts.level}`;
		return this.get<ListResponse<Issue>>(path);
	}

	async getIssue(issueId: string): Promise<Issue> {
		return this.get<Issue>(`/issues/${issueId}`);
	}

	async setIssueStatus(issueId: string, status: IssueStatus): Promise<void> {
		await this.post(`/issues/${issueId}/status`, { status });
	}

	// ── Search ────────────────────────────────────────────────────────

	async searchIssues(
		projectSlug: string,
		opts?: { q: string; page?: number; perPage?: number; status?: string; level?: string }
	): Promise<ListResponse<Issue>> {
		const page = opts?.page ?? 1;
		const perPage = opts?.perPage ?? 20;
		let path = `/projects/${projectSlug}/search?q=${encodeURIComponent(opts?.q ?? '')}&page=${page}&per_page=${perPage}`;
		if (opts?.status) path += `&status=${opts.status}`;
		if (opts?.level) path += `&level=${opts.level}`;
		return this.get<ListResponse<Issue>>(path);
	}

	// ── Events ────────────────────────────────────────────────────────

	async listEvents(
		issueId: string,
		page = 1,
		perPage = 20
	): Promise<ListResponse<StoredEvent>> {
		return this.get<ListResponse<StoredEvent>>(
			`/issues/${issueId}/events?page=${page}&per_page=${perPage}`
		);
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

/** Redirect to login page (used on 401 responses). */
function gotoLogin() {
	if (typeof window !== 'undefined' && !window.location.pathname.startsWith('/setup')) {
		window.location.href = '/login';
	}
}

// ── Alert Rules ────────────────────────────────────────────────────────

export interface AlertRule {
	id: string;
	project_id: string;
	name: string;
	enabled: boolean;
	conditions: Record<string, unknown>;
	action_type: string;
	action_config: Record<string, unknown>;
	cooldown_seconds: number;
	created_at: string;
	updated_at: string;
}

export interface CreateAlertRule {
	name: string;
	conditions: Record<string, unknown>;
	action_type?: string;
	action_config?: Record<string, unknown>;
	cooldown_seconds?: number;
}

// ── Standalone API Functions (not yet on ApiClient) ─────────────────────
// These use raw fetch to match the existing auth pattern.
// TODO: migrate into ApiClient class methods.

export async function listAlertRules(projectSlug: string): Promise<AlertRule[]> {
	const res = await fetch(`${API_BASE}/projects/${projectSlug}/rules`);
	if (res.status === 401) { gotoLogin(); throw new ApiClientError(401, 'Not authenticated'); }
	if (!res.ok) throw new ApiClientError(res.status, await res.text());
	return res.json();
}

export async function createAlertRule(
	projectSlug: string,
	rule: CreateAlertRule
): Promise<AlertRule> {
	const res = await fetch(`${API_BASE}/projects/${projectSlug}/rules`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(rule)
	});
	if (res.status === 401) { gotoLogin(); throw new ApiClientError(401, 'Not authenticated'); }
	if (!res.ok) throw new ApiClientError(res.status, await res.text());
	return res.json();
}

export async function deleteAlertRule(ruleId: string): Promise<void> {
	const res = await fetch(`${API_BASE}/rules/${ruleId}`, { method: 'DELETE' });
	if (res.status === 401) { gotoLogin(); throw new ApiClientError(401, 'Not authenticated'); }
	if (!res.ok && res.status !== 200) throw new ApiClientError(res.status, await res.text());
}

export async function toggleAlertRule(ruleId: string, enabled: boolean): Promise<void> {
	const res = await fetch(`${API_BASE}/rules/${ruleId}/toggle`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ enabled })
	});
	if (res.status === 401) { gotoLogin(); throw new ApiClientError(401, 'Not authenticated'); }
	if (!res.ok) throw new ApiClientError(res.status, await res.text());
}

export const api = new ApiClient();
