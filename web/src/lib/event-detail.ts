/**
 * Defensive parsers for the stored event JSON. Sentry events vary a lot between
 * SDKs, so every helper tolerates missing, null or wrongly typed fields.
 */

export type Dict = Record<string, unknown>;

export interface Frame {
	filename: string;
	function: string;
	module: string;
	lineno: number | null;
	colno: number | null;
	inApp: boolean;
	preContext: string[];
	contextLine: string | null;
	postContext: string[];
}

export interface ParsedException {
	type: string;
	value: string;
	frames: Frame[];
}

export interface ParsedBreadcrumb {
	timestamp: string | null;
	type: string;
	category: string;
	message: string;
	level: string;
	data: unknown;
}

export interface KeyValue {
	key: string;
	value: string;
}

export function isDict(v: unknown): v is Dict {
	return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function str(v: unknown): string {
	return typeof v === 'string' ? v : v == null ? '' : String(v);
}

function num(v: unknown): number | null {
	return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

function strArray(v: unknown): string[] {
	return Array.isArray(v) ? v.map(str) : [];
}

/** Sentry wraps lists as `{ values: [...] }`; some SDKs send a bare array. */
function unwrapValues(v: unknown): unknown[] {
	if (Array.isArray(v)) return v;
	if (isDict(v) && Array.isArray(v.values)) return v.values;
	return [];
}

/** Render any value as a short human string. Objects become compact JSON. */
export function formatValue(v: unknown): string {
	if (v == null) return '';
	if (typeof v === 'string') return v;
	if (typeof v === 'number' || typeof v === 'boolean') return String(v);
	try {
		return JSON.stringify(v);
	} catch {
		return String(v);
	}
}

export function parseFrame(raw: unknown): Frame | null {
	if (!isDict(raw)) return null;
	const filename = str(raw.filename) || str(raw.abs_path);
	return {
		filename,
		function: str(raw.function),
		module: str(raw.module),
		lineno: num(raw.lineno),
		colno: num(raw.colno),
		inApp: raw.in_app === true,
		preContext: strArray(raw.pre_context),
		contextLine: typeof raw.context_line === 'string' ? raw.context_line : null,
		postContext: strArray(raw.post_context)
	};
}

/**
 * Parse exceptions. Frames are returned most recent call first (Sentry sends
 * them oldest first), which is the order developers read them in.
 */
export function parseExceptions(data: Dict | null | undefined): ParsedException[] {
	if (!data) return [];
	return unwrapValues(data.exception)
		.filter(isDict)
		.map((e) => {
			const st = isDict(e.stacktrace) ? e.stacktrace : null;
			const rawFrames: unknown[] = st && Array.isArray(st.frames) ? st.frames : [];
			const frames = rawFrames
				.map(parseFrame)
				.filter((f): f is Frame => f !== null)
				.reverse();
			return { type: str(e.type) || 'Error', value: str(e.value), frames };
		});
}

/** Label for a frame location, e.g. `src/app.ts:12:4`. */
export function frameLocation(f: Frame): string {
	const base = f.filename || f.module || '<unknown>';
	if (f.lineno == null) return base;
	return f.colno == null ? `${base}:${f.lineno}` : `${base}:${f.lineno}:${f.colno}`;
}

/** Group consecutive library frames so they can be collapsed. */
export type FrameGroup = { collapsed: boolean; frames: Frame[] };

export function groupFrames(frames: Frame[]): FrameGroup[] {
	const groups: FrameGroup[] = [];
	for (const f of frames) {
		const collapsed = !f.inApp;
		const last = groups[groups.length - 1];
		if (last && last.collapsed === collapsed) last.frames.push(f);
		else groups.push({ collapsed, frames: [f] });
	}
	// A trace with no in-app frames at all should not be hidden entirely.
	if (groups.length > 0 && groups.every((g) => g.collapsed)) {
		groups.forEach((g) => (g.collapsed = false));
	}
	return groups;
}

/** Breadcrumbs in the order received (oldest first). */
export function parseBreadcrumbs(data: Dict | null | undefined): ParsedBreadcrumb[] {
	if (!data) return [];
	return unwrapValues(data.breadcrumbs)
		.filter(isDict)
		.map((b) => ({
			timestamp:
				typeof b.timestamp === 'string'
					? b.timestamp
					: typeof b.timestamp === 'number'
						? new Date(b.timestamp * 1000).toISOString()
						: null,
			type: str(b.type) || 'default',
			category: str(b.category),
			message: str(b.message),
			level: str(b.level) || 'info',
			data: b.data ?? null
		}));
}

/** Tags arrive as an object or as `[[key, value], ...]`. */
export function parseTags(data: Dict | null | undefined): KeyValue[] {
	const raw = data?.tags;
	const out: KeyValue[] = [];
	if (Array.isArray(raw)) {
		for (const item of raw) {
			if (Array.isArray(item) && item.length >= 2) out.push({ key: str(item[0]), value: formatValue(item[1]) });
			else if (isDict(item) && 'key' in item) out.push({ key: str(item.key), value: formatValue(item.value) });
		}
	} else if (isDict(raw)) {
		for (const [key, value] of Object.entries(raw)) out.push({ key, value: formatValue(value) });
	}
	return out;
}

/** Flatten one level of an object into key/value rows (nested values as JSON). */
export function toRows(v: unknown): KeyValue[] {
	if (!isDict(v)) return [];
	return Object.entries(v)
		.filter(([, val]) => val !== null && val !== undefined && val !== '')
		.map(([key, val]) => ({ key, value: formatValue(val) }));
}

export interface ParsedRequest {
	method: string;
	url: string;
	headers: KeyValue[];
	query: string;
	rows: KeyValue[];
}

export function parseRequest(data: Dict | null | undefined): ParsedRequest | null {
	const r = data?.request;
	if (!isDict(r) || Object.keys(r).length === 0) return null;
	let headers: KeyValue[] = [];
	if (isDict(r.headers)) headers = toRows(r.headers);
	else if (Array.isArray(r.headers))
		headers = r.headers
			.filter((h): h is unknown[] => Array.isArray(h))
			.map((h) => ({ key: str(h[0]), value: formatValue(h[1]) }));
	const query =
		typeof r.query_string === 'string'
			? r.query_string
			: isDict(r.query_string)
				? new URLSearchParams(r.query_string as Record<string, string>).toString()
				: '';
	const rows = toRows({ env: r.env, cookies: r.cookies, data: r.data }).filter((x) => x.value !== '{}');
	return { method: str(r.method).toUpperCase(), url: str(r.url), headers, query, rows };
}

export function parseUser(data: Dict | null | undefined): KeyValue[] {
	const ctx = data?.contexts;
	const u = data?.user ?? (isDict(ctx) ? ctx.user : undefined);
	return toRows(u);
}

/** Contexts keyed by name (os, device, runtime, ...), each flattened to rows. */
export function parseContexts(data: Dict | null | undefined): { name: string; rows: KeyValue[] }[] {
	const c = data?.contexts;
	if (!isDict(c)) return [];
	return Object.entries(c)
		.map(([name, v]) => {
			const rows = toRows(isDict(v) ? v : { value: v }).filter((r) => r.key !== 'type');
			return { name, rows };
		})
		.filter((ctx) => ctx.rows.length > 0);
}

/** Top-level facts: release, environment, platform, server name, SDK. */
export function parseSummary(data: Dict | null | undefined): KeyValue[] {
	if (!data) return [];
	const sdk = isDict(data.sdk) ? [str(data.sdk.name), str(data.sdk.version)].filter(Boolean).join(' ') : '';
	return toRows({
		release: data.release,
		environment: data.environment,
		platform: data.platform,
		server_name: data.server_name,
		sdk
	});
}

export function parseExtra(data: Dict | null | undefined): KeyValue[] {
	return toRows(data?.extra);
}

/** Resolve neighbours in a newest-first list. `newer` precedes, `older` follows. */
export function neighbours<T extends { id: string }>(
	list: T[],
	id: string
): { index: number; newer: T | null; older: T | null } {
	const index = list.findIndex((e) => e.id === id);
	if (index < 0) return { index, newer: null, older: null };
	return { index, newer: list[index - 1] ?? null, older: list[index + 1] ?? null };
}
