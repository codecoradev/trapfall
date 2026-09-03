import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';
import type { Snippet } from 'svelte';
import { formatInTimezone, activeTimezone } from './timezone';

// ── Shadcn UI helpers ─────────────────────────────────────────────────

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export type WithoutChildren<T> = T extends { children?: Snippet } ? Omit<T, 'children'> : T;
export type WithoutChild<T> = T extends { child?: Snippet } ? Omit<T, 'child'> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = WithoutChildren<T> & {
	ref?: U | null;
	children?: Snippet;
};

// ── Badge variant types ───────────────────────────────────────────────

export type BadgeVariant = 'destructive' | 'secondary' | 'outline' | 'default';

// ── Badge color mapping ───────────────────────────────────────────────

/**
 * Map error level to badge variant for consistent coloring across the dashboard.
 */
export function levelColor(level: string): BadgeVariant {
	const map: Record<string, BadgeVariant> = {
		fatal: 'destructive',
		error: 'destructive',
		warning: 'secondary',
		info: 'outline',
		debug: 'outline'
	};
	return map[level] ?? 'outline';
}

/**
 * Map issue status to badge variant for consistent coloring across the dashboard.
 */
export function statusColor(status: string): BadgeVariant {
	const map: Record<string, BadgeVariant> = {
		unresolved: 'destructive',
		resolved: 'outline',
		ignored: 'secondary'
	};
	return map[status] ?? 'default';
}

// ── Tailwind text color mapping ───────────────────────────────────────

/**
 * Map error level to Tailwind text color class.
 */
export function levelTextClass(level: string): string {
	const map: Record<string, string> = {
		fatal: 'text-destructive',
		error: 'text-destructive',
		warning: 'text-warning',
		info: 'text-info',
		debug: 'text-muted-foreground',
		trace: 'text-muted-foreground'
	};
	return map[level] ?? 'text-muted-foreground';
}

/**
 * Map issue status to Tailwind text color class.
 */
export function statusTextClass(status: string): string {
	const map: Record<string, string> = {
		unresolved: 'text-destructive',
		resolved: 'text-success',
		ignored: 'text-muted-foreground'
	};
	return map[status] ?? 'text-muted-foreground';
}

// ── Time formatting ───────────────────────────────────────────────────

/**
 * Format an ISO date string to locale string.
 */
export function formatTime(iso: string): string {
	if (!iso) return '';
	// Use the active display timezone from the store (server-configured),
	// falling back to the browser zone if the store hasn't loaded yet.
	return formatInTimezone(iso, activeTimezone());
}

/**
 * Format an ISO date string as relative time (e.g. "5 minutes ago").
 */
export function timeAgo(dateStr: string): string {
	if (!dateStr) return '';
	const now = new Date();
	const date = new Date(dateStr);
	const seconds = Math.floor((now.getTime() - date.getTime()) / 1000);

	if (seconds < 60) return 'just now';
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) return `${minutes}m ago`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours}h ago`;
	const days = Math.floor(hours / 24);
	if (days < 30) return `${days}d ago`;
	const months = Math.floor(days / 30);
	if (months < 12) return `${months}mo ago`;
	return `${Math.floor(months / 12)}y ago`;
}

// ── Duration formatting ───────────────────────────────────────────────

/**
 * Format milliseconds to human-readable string (e.g. "1.23s", "45ms").
 */
export function formatDuration(ms: number): string {
	if (ms < 1) return '<1ms';
	if (ms < 1000) return `${Math.round(ms)}ms`;
	return `${(ms / 1000).toFixed(2)}s`;
}

/**
 * Map transaction status to Tailwind text color class.
 */
export function transactionStatusTextClass(status: string): string {
	const map: Record<string, string> = {
		ok: 'text-success',
		deadline_exceeded: 'text-destructive',
		cancelled: 'text-warning',
		unknown: 'text-muted-foreground'
	};
	return map[status] ?? 'text-muted-foreground';
}

/**
 * Map crash rate to Tailwind text color class.
 */
export function crashRateColor(rate: number | null): string {
	if (rate === null) return "";
	if (rate < 1) return "text-success";
	if (rate < 5) return "text-warning";
	return "text-destructive";
}

/**
 * Map crash rate to a bar fill color class (matches crashRateColor thresholds).
 */
export function crashRateBarClass(rate: number | null): string {
	if (rate === null) return "";
	if (rate < 1) return "bg-success";
	if (rate < 5) return "bg-warning";
	return "bg-destructive";
}
