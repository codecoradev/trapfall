<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type StoredEvent, type Issue, type AttachmentItem } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import AttachmentCard from '$lib/components/AttachmentCard.svelte';
	import EventDetail from '$lib/components/EventDetail.svelte';
	import { neighbours } from '$lib/event-detail';

	import { levelTextClass, statusTextClass, timeAgo } from '$lib/utils';

	let event: StoredEvent | null = $state(null);
	let issue: Issue | null = $state(null);
	let attachments: AttachmentItem[] = $state([]);
	let attachmentsLoading = $state(false);
	let loading = $state(true);
	let error = $state('');

	let projectSlug = $state('');
	let events: StoredEvent[] = $state([]);
	let issueId = '';

	const currentId: string | null = $derived((event as StoredEvent | null)?.id ?? null);
	const nav: { index: number; newer: StoredEvent | null; older: StoredEvent | null } = $derived(
		currentId ? neighbours(events, currentId) : { index: -1, newer: null, older: null }
	);

	function eventHref(id: string) {
		const qs = projectSlug ? '?project=' + projectSlug : '';
		return `/issues/${issueId}/events/${id}${qs}`;
	}

	async function loadAttachments(id: string) {
		attachmentsLoading = true;
		try {
			attachments = await api.listAttachments(id);
		} catch {
			// Silently handle attachment fetch errors
			attachments = [];
		} finally {
			attachmentsLoading = false;
		}
	}

	async function showEvent(eventId: string) {
		const found = events.find((e) => e.id === eventId) || null;
		if (!found) {
			error = 'Event not found';
			return;
		}
		event = found;
		await loadAttachments(found.id);
	}

	async function go(target: StoredEvent | null) {
		if (!target) return;
		await goto(eventHref(target.id), { replaceState: false, noScroll: true });
		await showEvent(target.id);
	}

	function goBack() {
		if (projectSlug) {
			goto('/issues?project=' + projectSlug);
		} else {
			goto('/issues');
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		const el = e.target as HTMLElement | null;
		if (el && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName))) return;
		if (e.metaKey || e.ctrlKey || e.altKey) return;
		if (e.key === 'Escape') {
			goBack();
		}
	}

	onMount(async () => {
		const id = page.params.issueId;
		const requested = page.params.eventId;
		if (!id || !requested) return;
		issueId = id;

		projectSlug = page.url.searchParams.get('project') || '';

		try {
			// Load issue and events in parallel (events are newest first)
			const [issueData, eventsRes] = await Promise.all([api.getIssue(id), api.listEvents(id, 1, 100)]);

			issue = issueData;
			events = eventsRes.data;

			// "latest" is a valid event id: resolve to the newest event.
			const targetId = requested === 'latest' ? events[0]?.id : requested;
			const found = events.find((e) => e.id === targetId) || null;
			if (!found) {
				error = 'Event not found';
				loading = false;
				return;
			}
			event = found;
			if (requested === 'latest') {
				await goto(eventHref(found.id), { replaceState: true, noScroll: true });
			}
			await loadAttachments(found.id);
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

		<!-- Prev / next event navigation -->
		<nav class="flex flex-wrap items-center gap-2" aria-label="Event navigation">
			<Button variant="outline" size="sm" disabled={!nav.older} onclick={() => go(nav.older)} aria-label="Older event">
				&#x2190; Older
			</Button>
			<Button variant="outline" size="sm" disabled={!nav.newer} onclick={() => go(nav.newer)} aria-label="Newer event">
				Newer &#x2192;
			</Button>
			<Button variant="outline" size="sm" disabled={!events[0] || events[0].id === event.id} onclick={() => go(events[0] ?? null)}>
				Latest
			</Button>
			{#if nav.index >= 0}
				<span class="text-xs text-muted-foreground" aria-live="polite">
					Event {events.length - nav.index} of {events.length}{issue.count > events.length ? ` loaded (${issue.count} total)` : ''}
				</span>
			{/if}
		</nav>

		<!-- Event Data -->
		{#key event.id}
			<EventDetail data={event.data} />
		{/key}

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
