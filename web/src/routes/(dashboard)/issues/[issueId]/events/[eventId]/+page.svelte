<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type StoredEvent, type Issue } from '$lib/api';
	import { fetchAttachments, type AttachmentItem } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import AttachmentCard from '$lib/components/AttachmentCard.svelte';

	import { levelTextClass, statusTextClass, timeAgo } from '$lib/utils';

	let event: StoredEvent | null = $state(null);
	let issue: Issue | null = $state(null);
	let attachments: AttachmentItem[] = $state([]);
	let attachmentsLoading = $state(false);
	let loading = $state(true);
	let error = $state('');

	let projectSlug = $state('');

	function goBack() {
		if (projectSlug) {
			goto('/issues?project=' + projectSlug);
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
		const issueId = page.params.issueId;
		const eventId = page.params.eventId;
		if (!issueId || !eventId) return;

		projectSlug = page.url.searchParams.get('project') || '';

		try {
			// Load issue and events in parallel
			const [issueData, eventsRes] = await Promise.all([
				api.getIssue(issueId),
				api.listEvents(issueId)
			]);

			issue = issueData;
			event = eventsRes.data.find((e) => e.id === eventId) || null;

			if (!event) {
				error = 'Event not found';
				loading = false;
				return;
			}

			// Fetch attachments for this event
			attachmentsLoading = true;
			try {
				attachments = await fetchAttachments(eventId);
			} catch {
				// Silently handle attachment fetch errors
				attachments = [];
			} finally {
				attachmentsLoading = false;
			}
		} catch (e: any) {
			error = e?.message || 'Failed to load event';
		} finally {
			loading = false;
		}
	});
</script>

<svelte:head>
	<title>{event ? 'Event' : 'Event'} - TrapFall</title>
</svelte:head>

<svelte:window onkeydown={handleKeydown} />

<div class="p-4 lg:p-6 space-y-4">
	{#if loading}
		<div class="space-y-4">
			<Skeleton class="h-8 w-1/2" />
			<Skeleton class="h-48 w-full" />
			<Skeleton class="h-32 w-full" />
		</div>
	{:else if error}
		<div class="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
			<p class="text-sm text-destructive">{error}</p>
		</div>
	{:else if issue && event}
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

		<!-- Event Header -->
		<div class="space-y-1">
			<div class="flex items-center gap-2">
				<Badge variant="outline" class={levelTextClass(issue.level)}>
					{issue.level}
				</Badge>
				<Badge variant="outline" class={statusTextClass(issue.status)}>
					{issue.status}
				</Badge>
				<span class="text-sm text-muted-foreground">
					Event received {timeAgo(event.received_at)}
				</span>
			</div>
			<h1 class="text-xl font-bold">{issue.title}</h1>
		</div>

		<!-- Event Data -->
		<Card>
			<CardHeader class="pb-2">
				<CardTitle class="text-sm font-mono">
					Event Data
				</CardTitle>
			</CardHeader>
			<CardContent>
				{#if event.data?.message}
					<p class="text-sm mb-2">{event.data.message as string}</p>
				{/if}
				<pre class="text-xs bg-muted p-3 rounded overflow-auto max-h-96">{JSON.stringify(event.data, null, 2)}</pre>
			</CardContent>
		</Card>

		<!-- Attachments Section -->
		{#if attachmentsLoading}
			<div class="space-y-3">
				<h2 class="text-lg font-semibold">Attachments</h2>
				<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
					{#each Array(3) as _}
						<Skeleton class="h-64 w-full rounded-xl" />
					{/each}
				</div>
			</div>
		{:else if attachments.length > 0}
			<div class="space-y-3">
				<h2 class="text-lg font-semibold">Attachments ({attachments.length})</h2>
				<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
					{#each attachments as attachment (attachment.id)}
						<AttachmentCard {attachment} />
					{/each}
				</div>
			</div>
		{/if}
	{/if}
</div>
