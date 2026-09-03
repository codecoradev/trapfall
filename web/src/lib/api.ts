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
	archived_at?: string;
}

export interface SetupStatus {
	needs_setup: boolean;
}

export interface PublicConfig {
	/** IANA timezone name, e.g. "Asia/Jakarta" or "UTC". Display only. */
	timezone: string;
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

export class ApiClientError extends Error {
	constructor(
		public status: number,
		message: string
	) {
		super(message);
		this.name = 'ApiClientError';
	}
}

// ── Transactions ──────────────────────────────────────────────────────

export interface TransactionResponse {
	id: string;
	name: string;
	release: string | null;
	environment: string | null;
	duration_ms: number;
	status: string;
	received_at: string;
}

export interface SpanResponse {
	span_id: string;
	trace_id: string;
	parent_span_id: string | null;
	op: string | null;
	description: string | null;
	start_offset_ms: number;
	duration_ms: number;
	status: string | null;
}

export interface TransactionDetailResponse extends TransactionResponse {
	spans: SpanResponse[];

}
// ── Release Health ───────────────────────────────────────────────────

export interface ReleaseHealthResponse {
	id: string;
	release: string;
	environment: string | null;
	started_at: string;
	distinct_id: string | null;
	exited: number;
	errored: number;
	abnormal: number;
	crashed: number;
	crash_rate: number | null;
	received_at: string;
}

export interface CrashRateResponse {
	crash_rate: number;
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

	async updateProject(slug: string, name: string): Promise<Project> {
		return this.request<Project>(`/projects/${slug}`, {
			method: 'PATCH',
			body: JSON.stringify({ name })
		});
	}

	async deleteProject(slug: string): Promise<void> {
		await this.request<void>(`/projects/${slug}`, { method: 'DELETE' });
	}

	async archiveProject(slug: string): Promise<void> {
		await this.post(`/projects/${slug}/archive`);
	}

	async unarchiveProject(slug: string): Promise<void> {
		await this.request<void>(`/projects/${slug}/archive`, { method: 'DELETE' });
	}

	async rotateDsn(slug: string): Promise<Project> {
		return this.post<Project>(`/projects/${slug}/rotate-dsn`);
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
	// ── Transactions ──────────────────────────────────────────────────────

	async listTransactions(projectSlug: string, page = 1, perPage = 20): Promise<ListResponse<TransactionResponse>> {
		return this.get<ListResponse<TransactionResponse>>(`/projects/${projectSlug}/transactions?page=${page}&per_page=${perPage}`);
	}

	async getTransaction(projectSlug: string, txnId: string): Promise<TransactionDetailResponse> {
		return this.get<TransactionDetailResponse>(`/projects/${projectSlug}/transactions/${txnId}`);
	}

	async getSlowestTransactions(projectSlug: string, limit = 5): Promise<TransactionResponse[]> {
		return this.get<TransactionResponse[]>(`/projects/${projectSlug}/transactions/slowest?limit=${limit}`);
	}

	// ── Release Health ─────────────────────────────────────────────────

	async listReleaseHealth(projectSlug: string, opts?: { page?: number; perPage?: number; release?: string; env?: string }): Promise<ListResponse<ReleaseHealthResponse>> {
		const page = opts?.page ?? 1;
		const perPage = opts?.perPage ?? 20;
		let path = `/projects/${projectSlug}/release-health?page=${page}&per_page=${perPage}`;
		if (opts?.release) path += `&release=${encodeURIComponent(opts.release)}`;
		if (opts?.env) path += `&env=${encodeURIComponent(opts.env)}`;
		return this.get<ListResponse<ReleaseHealthResponse>>(path);
	}

	async getCrashRate(projectSlug: string, release?: string, env?: string): Promise<CrashRateResponse> {
		let path = `/projects/${projectSlug}/release-health/crash-rate`;
		if (release) path += `?release=${encodeURIComponent(release)}`;
		if (env) path += `&env=${encodeURIComponent(env)}`;
		return this.get<CrashRateResponse>(path);
	}

	async listEnvironments(projectSlug: string): Promise<string[]> {
		return this.get<string[]>(`/projects/${projectSlug}/environments`);
	}

	// ── Alert Rules ─────────────────────────────────────────────────

	async listAlertRules(projectSlug: string): Promise<AlertRule[]> {
		return this.get<AlertRule[]>(`/projects/${projectSlug}/rules`);
	}

	async createAlertRule(projectSlug: string, rule: CreateAlertRule): Promise<AlertRule> {
		return this.post<AlertRule>(`/projects/${projectSlug}/rules`, rule);
	}

	async deleteAlertRule(ruleId: string): Promise<void> {
		await this.delete(`/rules/${ruleId}`);
	}

	async toggleAlertRule(ruleId: string, enabled: boolean): Promise<void> {
		await this.post(`/rules/${ruleId}/toggle`, { enabled });
	}

	// ── Attachments ─────────────────────────────────────────────────

	async listAttachments(eventId: string): Promise<AttachmentItem[]> {
		const data = await this.get<{ items?: AttachmentItem[] }>(`/events/${eventId}/attachments`);
		return data.items || [];
	}

	async getPublicConfig(): Promise<PublicConfig> {
		return this.get<PublicConfig>('/config');
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

// ── Standalone API helpers ──────────────────────────────────────────────

export const api = new ApiClient();


// ── Attachments ──────────────────────────────────────────────────────

export interface AttachmentItem {
	id: string;
	filename: string;
	content_type: string | null;
	attachment_type: string | null;
	size_bytes: number;
	created_at: string;
}

export function getAttachmentDownloadUrl(attachmentId: string): string {
	return `${API_BASE}/attachments/${attachmentId}/download`;
}
