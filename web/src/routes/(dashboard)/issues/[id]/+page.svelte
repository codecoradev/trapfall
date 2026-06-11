<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api, type Issue, type StoredEvent, type Project } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';

	let issue: Issue | null = $state(null);
	let events: StoredEvent[] = $state([]);
	let project: Project | null = $state(null);
	let loading = $state(true);
	let error = $state('');

	import { levelTextClass, statusTextClass, timeAgo } from '$lib/utils';

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

	function goBack() {
		const projectSlug = page.url.searchParams.get('project');
		if (projectSlug) {
			goto(`/issues?project=${projectSlug}`);
		} else {
			goto('/issues');
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			goBack();
		}
	}

	onMount(async () => {
		const issueId = page.params.id;
		if (!issueId) return;

		try {
			issue = await api.getIssue(issueId);
			const res = await api.listEvents(issueId);
			events = res.data;

			// Load project info
			const projectSlug = page.url.searchParams.get('project');
			if (projectSlug) {
				try {
					project = await api.getProject(projectSlug);
				} catch {
					// Project lookup failed, that's OK
				}
			} else {
				// Try to find project from projects list
				const projects = await api.listProjects();
				const match = projects.find(p => p.id === issue?.project_id);
				if (match) project = match;
			}
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
			<Skeleton class="h-8 w-1/2" />
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
				←
				Back
			</Button>
			{#if project}
				<span class="text-muted-foreground/50">·</span>
				<Badge variant="outline" class="text-xs">{project.name}</Badge>
			{/if}
			<span class="text-muted-foreground/50">·</span>
			<span class="text-xs">Press <kbd class="rounded border px-1 py-0.5 text-[10px] font-mono">ESC</kbd> to go back</span>
		</div>

		<!-- Header -->
		<div class="flex items-start justify-between gap-4">
			<div class="space-y-1">
				<div class="flex items-center gap-2">
					<Badge variant="outline" class={levelTextClass(issue.level)}>
						{issue.level}
					</Badge>
					<Badge variant="outline" class={statusTextClass(issue.status)}>
						{issue.status}
					</Badge>
					<h1 class="text-xl font-bold">{issue.title}</h1>
				</div>
				<p class="text-sm text-muted-foreground">
					{issue.culprit || 'No culprit'} · {issue.count} events · {issue.user_count} users · Last seen {timeAgo(issue.last_seen)}
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
								{timeAgo(event.received_at)} · {event.platform || 'unknown'}
							</CardTitle>
						</CardHeader>
						<CardContent>
							{#if event.message}
								<p class="text-sm mb-2">{event.message}</p>
							{/if}
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
