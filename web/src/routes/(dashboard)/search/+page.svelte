<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type Issue } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';

	type BadgeVariant = 'destructive' | 'secondary' | 'outline' | 'default';

	let query = $state('');
	let results = $state<Issue[]>([]);
	let loading = $state(false);
	let searched = $state(false);
	let statusFilter = $state('');
	let levelFilter = $state('');

	let debounceTimer: ReturnType<typeof setTimeout>;

	function doSearch() {
		if (!query.trim()) return;
		loading = true;
		searched = true;

		const params = new URLSearchParams();
		params.set('q', query);
		params.set('limit', '50');
		if (statusFilter) params.set('status', statusFilter);
		if (levelFilter) params.set('level', levelFilter);

		api
			.get<{ data: Issue[] }>(`/api/0/projects/default/search?${params}`)
			.then((data) => {
				results = data.data ?? [];
			})
			.catch(() => {
				results = [];
			})
			.finally(() => {
				loading = false;
			});
	}

	function onInput() {
		clearTimeout(debounceTimer);
		debounceTimer = setTimeout(doSearch, 300);
	}

	function levelColor(level: string): BadgeVariant {
		const colors: Record<string, BadgeVariant> = {
			fatal: 'destructive',
			error: 'destructive',
			warning: 'secondary',
			info: 'outline',
			debug: 'outline'
		};
		return colors[level] ?? 'outline';
	}

	function statusVariant(status: string): BadgeVariant {
		if (status === 'resolved') return 'outline';
		if (status === 'ignored') return 'secondary';
		return 'default';
	}

	function highlight(text: string, q: string): string {
		if (!q) return text;
		const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
		return text.replace(new RegExp(`(${escaped})`, 'gi'), '<mark>$1</mark>');
	}
</script>

<svelte:head>
	<title>Search — TrapFall</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-center justify-between">
		<h1 class="text-2xl font-bold">Search</h1>
	</div>

	<div class="flex items-center gap-3">
		<div class="relative flex-1">
			<svg
				class="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground"
				xmlns="http://www.w3.org/2000/svg"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="2"
			>
				<circle cx="11" cy="11" r="8" />
				<path d="m21 21-4.3-4.3" />
			</svg>
			<input
				type="text"
				placeholder="Search errors by title or culprit..."
				bind:value={query}
				oninput={onInput}
				class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 pl-10 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
			/>
		</div>

		<select
			bind:value={statusFilter}
			onchange={doSearch}
			class="h-10 rounded-md border border-input bg-background px-3 text-sm"
		>
			<option value="">All Status</option>
			<option value="unresolved">Unresolved</option>
			<option value="resolved">Resolved</option>
			<option value="ignored">Ignored</option>
		</select>

		<select
			bind:value={levelFilter}
			onchange={doSearch}
			class="h-10 rounded-md border border-input bg-background px-3 text-sm"
		>
			<option value="">All Levels</option>
			<option value="fatal">Fatal</option>
			<option value="error">Error</option>
			<option value="warning">Warning</option>
			<option value="info">Info</option>
		</select>
	</div>

	{#if loading}
		<div class="text-center text-muted-foreground py-12">Searching...</div>
	{:else if searched && results.length === 0}
		<div class="text-center text-muted-foreground py-12">
			No results found for "{query}"
		</div>
	{:else if results.length > 0}
		<div class="space-y-2">
			{#each results as issue (issue.id)}
				<a
					href="/issues/{issue.id}"
					class="block rounded-lg border p-4 hover:bg-accent/50 transition-colors"
				>
					<div class="flex items-start justify-between gap-4">
						<div class="flex-1 min-w-0">
							<p class="font-medium truncate">
								{@html highlight(issue.title, query)}
							</p>
							{#if issue.culprit}
								<p class="text-sm text-muted-foreground truncate mt-1">
									{@html highlight(issue.culprit, query)}
								</p>
							{/if}
						</div>
						<div class="flex items-center gap-2 shrink-0">
							<Badge variant={levelColor(issue.level)}>{issue.level}</Badge>
							<Badge variant={statusVariant(issue.status)}>
								{issue.status}
							</Badge>
							<span class="text-xs text-muted-foreground whitespace-nowrap">
								×{issue.count}
							</span>
						</div>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
