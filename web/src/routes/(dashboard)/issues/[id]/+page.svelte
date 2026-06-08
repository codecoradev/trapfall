<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { api, type Issue, type StoredEvent } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';

	let issue: Issue | null = $state(null);
	let events: StoredEvent[] = $state([]);
	let loading = $state(true);
	let error = $state('');

	import { levelTextClass } from '$lib/utils';

	const statusLabels: Record<string, string> = {
		unresolved: 'Resolve',
		resolved: 'Reopen',
		ignored: 'Unignore'
	};

	async function toggleStatus() {
		if (!issue) return;
		const newStatus = issue.status === 'unresolved' ? 'resolved' : 'unresolved';
		try {
			await api.setIssueStatus(issue.id, newStatus);
			issue = { ...issue, status: newStatus };
		} catch (e: any) {
			error = e?.message || 'Failed to update status';
		}
	}

	onMount(async () => {
		const issueId = page.params.id;
		if (!issueId) return;

		try {
			issue = await api.getIssue(issueId);
			const res = await api.listEvents(issueId);
			events = res.data;
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

<div class="p-4 lg:p-6 space-y-4">
	{#if loading}
		<div class="space-y-4">
			<Skeleton class="h-8 w-1/2" />
			<Skeleton class="h-48 w-full" />
		</div>
	{:else if error}
		<div class="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
			<p class="text-sm text-destructive">{error}</p>
		</div>
	{:else if issue}
		<!-- Header -->
		<div class="flex items-start justify-between gap-4">
			<div class="space-y-1">
				<div class="flex items-center gap-2">
					<Badge variant="outline" class={levelTextClass(issue.level)}>
						{issue.level}
					</Badge>
					<h1 class="text-xl font-bold">{issue.title}</h1>
				</div>
				<p class="text-sm text-muted-foreground">
					{issue.culprit || 'No culprit'} · {issue.count} events · {issue.user_count} users
				</p>
			</div>
			<Button variant="outline" onclick={toggleStatus}>
				{statusLabels[issue.status] || 'Toggle'}
			</Button>
		</div>

		<!-- Event List -->
		<div class="space-y-3">
			<h2 class="text-lg font-semibold">Events</h2>
			{#if events.length === 0}
				<p class="text-sm text-muted-foreground">No event data available.</p>
			{:else}
				{#each events as event}
					<Card>
						<CardHeader class="pb-2">
							<CardTitle class="text-sm font-mono">
								{event.received_at}
							</CardTitle>
						</CardHeader>
						<CardContent>
							<pre class="text-xs bg-muted p-3 rounded overflow-auto max-h-96">
{JSON.stringify(event.data, null, 2)}</pre
							>
						</CardContent>
					</Card>
				{/each}
			{/if}
		</div>
	{/if}
</div>
