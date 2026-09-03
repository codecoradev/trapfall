<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type StoredEvent, type Issue, type IssueStatus } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent } from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Pagination from '$lib/components/Pagination.svelte';
	import Sparkline from '$lib/components/Sparkline.svelte';
	import {
		Table,
		TableBody,
		TableCell,
		TableHead,
		TableHeader,
		TableRow
	} from '$lib/components/ui/table/index.js';

	import { levelTextClass, statusTextClass, timeAgo, formatTime } from '$lib/utils';

	let issue: Issue | null = $state(null);
	let events: StoredEvent[] = $state([]);
	let totalEvents: number = $state(0);
	let eventsPage: number = $state(1);
	const eventsPerPage = 10;
	let loading = $state(true);
	let error = $state('');
	let statusError = $state('');
	let updatingStatus = $state(false);

	let projectSlug = $state('');

	let totalEventPages: number = $derived(Math.max(1, Math.ceil(totalEvents / eventsPerPage)));

	const BUCKET_DAYS = 14;
	let eventBuckets: number[] = $state([]);
	let bucketRange = $state('');

	/** Bucket event timestamps into per-day counts for the last N days. */
	function buildBuckets(sample: StoredEvent[]) {
		const start = new Date();
		start.setHours(0, 0, 0, 0);
		start.setDate(start.getDate() - (BUCKET_DAYS - 1));
		const counts = new Array<number>(BUCKET_DAYS).fill(0);
		for (const e of sample) {
			const idx = Math.floor((new Date(e.received_at).getTime() - start.getTime()) / 86400000);
			if (idx >= 0 && idx < BUCKET_DAYS) counts[idx] += 1;
		}
		eventBuckets = counts;
		const fmt = (d: Date) => d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
		bucketRange = `${fmt(start)} - ${fmt(new Date())}`;
	}

	/** Extract the first exception type/value from a Sentry-format event payload. */
	function exceptionSummary(event: StoredEvent): { type: string; value: string } | null {
		const ex = event.data?.exception as { values?: Array<{ type?: string; value?: string }> } | undefined;
		const first = ex?.values?.[0];
		if (!first) return null;
		return { type: first.type || 'Exception', value: first.value || '' };
	}

	function eventSummary(event: StoredEvent): string {
		const message = event.data?.message;
		if (typeof message === 'string' && message) return message;
		const ex = exceptionSummary(event);
		if (ex) return ex.value ? `${ex.type}: ${ex.value}` : ex.type;
		return 'Event';
	}

	function goBack() {
		if (projectSlug) {
			goto('/issues?project=' + projectSlug);
		} else {
			goto('/issues');
		}
	}

	function goToEvent(event: StoredEvent) {
		const qs = projectSlug ? `?project=${projectSlug}` : '';
		goto(`/issues/${issue!.id}/events/${event.id}${qs}`);
	}

	async function changeStatus(status: IssueStatus) {
		if (!issue || updatingStatus || issue.status === status) return;
		const prev = issue.status;
		issue = { ...issue, status };
		updatingStatus = true;
		statusError = '';
		try {
			await api.setIssueStatus(issue.id, status);
		} catch (e: any) {
			issue = { ...issue, status: prev };
			statusError = e?.message || 'Failed to update status';
		} finally {
			updatingStatus = false;
		}
	}

	async function loadEvents(p: number) {
		if (!issue) return;
		eventsPage = p;
		const res = await api.listEvents(issue.id, p, eventsPerPage);
		events = res.data;
		totalEvents = res.total;
	}

	function goToEventsPage(p: number) {
		if (p < 1 || p > totalEventPages) return;
		loadEvents(p);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			goBack();
		}
	}

	onMount(async () => {
		const issueId = page.params.issueId;
		if (!issueId) return;

		projectSlug = page.url.searchParams.get('project') || '';

		try {
			issue = await api.getIssue(issueId);
			await loadEvents(1);
			// Non-critical: sample up to 100 events for the daily trend chart.
			const sample = await api.listEvents(issueId, 1, 100).catch(() => null);
			if (sample) buildBuckets(sample.data);
		} catch (e: any) {
			error = e?.message || 'Failed to load issue';
		} finally {
			loading = false;
		}
	});
</script>

<svelte:head>
	<title>{issue ? issue.title : 'Issue'} · TrapFall</title>
</svelte:head>

<svelte:window onkeydown={handleKeydown} />

<div class="p-4 lg:p-6 space-y-4">
	{#if loading}
		<div class="space-y-4">
			<Skeleton class="h-6 w-32" />
			<Skeleton class="h-8 w-2/3" />
			<div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
				{#each Array(4) as _}
					<Skeleton class="h-20 w-full rounded-xl" />
				{/each}
			</div>
			<Skeleton class="h-48 w-full" />
		</div>
	{:else if error}
		<div class="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
			<p class="text-sm text-destructive">{error}</p>
		</div>
	{:else if issue}
		<!-- Back nav -->
		<div class="flex items-center gap-2 text-sm text-muted-foreground">
			<Button variant="ghost" size="sm" onclick={goBack} class="gap-1 px-2">
				&#x2190;
				Back
			</Button>
			{#if projectSlug}
				<span class="text-muted-foreground/50">&#xB7;</span>
				<Badge variant="outline" class="text-xs">{projectSlug}</Badge>
			{/if}
			<span class="text-muted-foreground/50">&#xB7;</span>
			<span class="text-xs">Press <kbd class="rounded border px-1 py-0.5 text-[10px] font-mono">ESC</kbd> to go back</span>
		</div>

		<!-- Header -->
		<div class="space-y-1">
			<div class="flex flex-wrap items-center gap-2">
				<Badge variant="outline" class={levelTextClass(issue.level)}>
					{issue.level}
				</Badge>
				<Badge variant="outline" class={statusTextClass(issue.status)}>
					{issue.status}
				</Badge>
			</div>
			<div class="flex flex-wrap items-start justify-between gap-3">
				<h1 class="text-xl font-bold min-w-0 break-words">{issue.title}</h1>
				<div class="flex items-center gap-2 shrink-0">
					{#if issue.status === 'resolved'}
						<Button variant="outline" size="sm" disabled={updatingStatus} onclick={() => changeStatus('unresolved')}>
							Unresolve
						</Button>
					{:else}
						<Button variant="outline" size="sm" disabled={updatingStatus} onclick={() => changeStatus('resolved')}>
							Resolve
						</Button>
					{/if}
					{#if issue.status === 'ignored'}
						<Button variant="outline" size="sm" disabled={updatingStatus} onclick={() => changeStatus('unresolved')}>
							Unignore
						</Button>
					{:else}
						<Button variant="outline" size="sm" disabled={updatingStatus} onclick={() => changeStatus('ignored')}>
							Ignore
						</Button>
					{/if}
				</div>
			</div>
			{#if issue.culprit}
				<p class="font-mono text-xs text-muted-foreground break-all">{issue.culprit}</p>
			{/if}
			{#if statusError}
				<p class="text-xs text-destructive">{statusError}</p>
			{/if}
		</div>

		<!-- Metadata stats -->
		<div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
			<Card>
				<CardContent class="p-4">
					<p class="text-xs font-medium text-muted-foreground">Events</p>
					<p class="text-2xl font-bold tabular-nums mt-1">{issue.count}</p>
				</CardContent>
			</Card>
			<Card>
				<CardContent class="p-4">
					<p class="text-xs font-medium text-muted-foreground">Users affected</p>
					<p class="text-2xl font-bold tabular-nums mt-1">{issue.user_count}</p>
				</CardContent>
			</Card>
			<Card>
				<CardContent class="p-4">
					<p class="text-xs font-medium text-muted-foreground">First seen</p>
					<p class="text-sm font-medium mt-1.5" title={formatTime(issue.first_seen)}>
						{timeAgo(issue.first_seen)}
					</p>
				</CardContent>
			</Card>
			<Card>
				<CardContent class="p-4">
					<p class="text-xs font-medium text-muted-foreground">Last seen</p>
					<p class="text-sm font-medium mt-1.5" title={formatTime(issue.last_seen)}>
						{timeAgo(issue.last_seen)}
					</p>
				</CardContent>
			</Card>
		</div>

		<!-- Events per day -->
		{#if eventBuckets.length > 0}
			<Card>
				<CardContent class="p-4 space-y-2">
					<div class="flex items-center justify-between">
						<p class="text-xs font-medium text-muted-foreground">Events, last {BUCKET_DAYS} days</p>
						<span class="text-xs text-muted-foreground tabular-nums">{bucketRange}</span>
					</div>
					<Sparkline values={eventBuckets} label="Events per day over the last {BUCKET_DAYS} days" />
				</CardContent>
			</Card>
		{/if}

		<!-- Events -->
		<div class="space-y-3">
			<div class="flex items-center justify-between">
				<h2 class="text-lg font-semibold">Events</h2>
				{#if totalEvents > 0}
					<span class="text-xs text-muted-foreground tabular-nums">{totalEvents} total</span>
				{/if}
			</div>

			{#if events.length === 0}
				<EmptyState
					title="No events recorded"
					description="Events for this issue will appear here as they arrive."
				/>
			{:else}
				<div class="rounded-lg border">
					<Table>
						<TableHeader>
							<TableRow>
								<TableHead class="w-[50%]">Summary</TableHead>
								<TableHead class="hidden md:table-cell">Exception</TableHead>
								<TableHead>Received</TableHead>
							</TableRow>
						</TableHeader>
						<TableBody>
							{#each events as event (event.id)}
								<TableRow
									class="cursor-pointer hover:bg-muted/50"
									onclick={() => goToEvent(event)}
								>
									<TableCell class="font-medium whitespace-normal">
										{eventSummary(event)}
									</TableCell>
									<TableCell class="hidden md:table-cell font-mono text-xs text-muted-foreground">
										{exceptionSummary(event)?.type || '—'}
									</TableCell>
									<TableCell class="text-muted-foreground text-sm whitespace-nowrap" title={formatTime(event.received_at)}>
										{timeAgo(event.received_at)}
									</TableCell>
								</TableRow>
							{/each}
						</TableBody>
					</Table>
				</div>

				<!-- Events pagination -->
				<Pagination
					page={eventsPage}
					totalPages={totalEventPages}
					total={totalEvents}
					perPage={eventsPerPage}
					onPageChange={goToEventsPage}
				/>
			{/if}
		</div>
	{/if}
</div>
