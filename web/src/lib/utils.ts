import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';
import type { Snippet } from 'svelte';

// ── Shadcn UI helpers ─────────────────────────────────────────────────

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export type WithoutChildren<T> = T extends { children?: Snippet } ? Omit<T, 'children'> : T;
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
		fatal: 'text-red-600',
		error: 'text-red-500',
		warning: 'text-yellow-500',
		info: 'text-blue-500',
		debug: 'text-gray-500',
		trace: 'text-gray-400'
	};
	return map[level] ?? 'text-gray-500';
}

/**
 * Map issue status to Tailwind text color class.
 */
export function statusTextClass(status: string): string {
	const map: Record<string, string> = {
		unresolved: 'text-red-500',
		resolved: 'text-green-600',
		ignored: 'text-gray-500'
	};
	return map[status] ?? 'text-gray-500';
}

// ── Time formatting ───────────────────────────────────────────────────

/**
 * Format an ISO date string to locale string.
 */
export function formatTime(iso: string): string {
	if (!iso) return '';
	return new Date(iso).toLocaleString();
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
		ok: 'text-green-600',
		deadline_exceeded: 'text-red-500',
		cancelled: 'text-yellow-500',
		unknown: 'text-gray-500'
	};
	return map[status] ?? 'text-gray-500';
}
