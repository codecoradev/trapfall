<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type Issue, type Project } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { levelColor, statusColor, formatTime } from '$lib/utils';

	let projects = $state<Project[]>([]);
	let selectedSlug = $state('');
	let query = $state('');
	let results = $state<Issue[]>([]);
	let total = $state(0);
	let page = $state(0);
	let loading = $state(false);
	let searched = $state(false);
	let statusFilter = $state('');
	let levelFilter = $state('');

	let debounceTimer: ReturnType<typeof setTimeout>;

	onMount(async () => {
		try {
			projects = await api.get<Project[]>('/0/projects');
			if (projects.length > 0) {
				selectedSlug = projects[0].slug;
			}
		} catch {
			projects = [];
		}
	});

	function doSearch() {
		if (!query.trim() || !selectedSlug) return;
		loading = true;
		searched = true;

		const params = new URLSearchParams();
		params.set('q', query);
		params.set('limit', '50');
		params.set('page', String(page));
		if (statusFilter) params.set('status', statusFilter);
		if (levelFilter) params.set('level', levelFilter);

		api
			.get<{ data: Issue[]; total: number }>(`/0/projects/${selectedSlug}/search?${params}`)
			.then((data) => {
				results = data.data ?? [];
				total = data.total ?? 0;
			})
			.catch(() => {
				results = [];
				total = 0;
			})
			.finally(() => {
				loading = false;
			});
	}

	function debouncedSearch() {
		clearTimeout(debounceTimer);
		debounceTimer = setTimeout(doSearch, 300);
	}
</script>

<div class="space-y-6 p-4 lg:p-6">
	<div>
		<h1 class="text-2xl font-bold">Search Issues</h1>
		<p class="text-muted-foreground">Search across all issues in a project</p>
	</div>

	<!-- Project selector + search bar -->
	<div class="flex gap-3">
		<select
			bind:value={selectedSlug}
			class="rounded-md border bg-background px-3 py-2 text-sm"
		>
			<option value="" disabled>Select project</option>
			{#each projects as p}
				<option value={p.slug}>{p.name} ({p.slug})</option>
			{/each}
		</select>

		<div class="flex-1">
			<input
				type="text"
				bind:value={query}
				oninput={debouncedSearch}
				onkeydown={(e) => e.key === 'Enter' && doSearch()}
				placeholder="Search by title or culprit..."
				class="w-full rounded-md border bg-background px-3 py-2 text-sm"
			/>
		</div>
	</div>

	<!-- Filters -->
	<div class="flex gap-3">
		<select bind:value={statusFilter} onchange={doSearch} class="rounded-md border bg-background px-3 py-2 text-sm">
			<option value="">All statuses</option>
			<option value="unresolved">Unresolved</option>
			<option value="resolved">Resolved</option>
			<option value="ignored">Ignored</option>
		</select>
		<select bind:value={levelFilter} onchange={doSearch} class="rounded-md border bg-background px-3 py-2 text-sm">
			<option value="">All levels</option>
			<option value="fatal">Fatal</option>
			<option value="error">Error</option>
			<option value="warning">Warning</option>
			<option value="info">Info</option>
			<option value="debug">Debug</option>
		</select>
	</div>

	<!-- Results -->
	{#if loading}
		<p class="text-muted-foreground">Searching...</p>
	{:else if searched && results.length === 0}
		<p class="text-muted-foreground">No results found.</p>
	{:else if results.length > 0}
		<div class="text-sm text-muted-foreground mb-2">{total} result{total !== 1 ? 's' : ''} found</div>
		<div class="space-y-2">
			{#each results as issue}
				<a
					href="/issues/{issue.id}"
					class="block rounded-lg border p-4 hover:bg-accent transition-colors"
				>
					<div class="flex items-center justify-between gap-2">
						<span class="font-medium truncate">{issue.title}</span>
						<div class="flex gap-1 shrink-0">
							<Badge variant={statusColor(issue.status)}>{issue.status}</Badge>
							<Badge variant={levelColor(issue.level)}>{issue.level}</Badge>
						</div>
					</div>
					<div class="text-sm text-muted-foreground mt-1">
						{issue.culprit ?? 'Unknown'} · {issue.count} events · Last seen {formatTime(issue.last_seen)}
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
