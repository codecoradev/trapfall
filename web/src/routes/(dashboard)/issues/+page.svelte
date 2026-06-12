<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type Issue, type Project } from '$lib/api';
	import { getWsClient, type ServerMessage } from '$lib/ws';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import {
		Table,
		TableBody,
		TableCell,
		TableHead,
		TableHeader,
		TableRow
	} from '$lib/components/ui/table/index.js';

	let issues: Issue[] = $state([]);
	let projects: Project[] = $state([]);
	let selectedProject: string = $state('');
	let filterStatus: string = $state('');
	let filterLevel: string = $state('');
	let currentPage: number = $state(1);
	let totalIssues: number = $state(0);
	const perPage = 20;
	let totalPages: number = $derived(Math.max(1, Math.ceil(totalIssues / perPage)));
	let loading = $state(true);
	let error = $state('');
	let liveIndicator = $state(false);

	import { levelTextClass, statusTextClass, timeAgo } from '$lib/utils';

	let wsUnsub: (() => void) | null = $state(null);

	let searchQuery: string = $state('');
	let debounceTimer: ReturnType<typeof setTimeout> | null = null;

	const statuses = ['unresolved', 'resolved', 'ignored'];
	const levels = ['fatal', 'error', 'warning', 'info', 'debug'];

	function buildUrl(): string {
		const params = new URLSearchParams();
		if (selectedProject) params.set('project', selectedProject);
		if (filterStatus) params.set('status', filterStatus);
		if (filterLevel) params.set('level', filterLevel);
		if (searchQuery.trim()) params.set('q', searchQuery.trim());
		if (currentPage > 1) params.set('page', String(currentPage));
		return `/issues?${params.toString()}`;
	}

	async function loadIssues() {
		if (!selectedProject) return;
		loading = true;
		error = '';
		try {
			if (searchQuery.trim()) {
				const res = await api.searchIssues(selectedProject, {
					q: searchQuery.trim(),
					page: currentPage,
					perPage,
					status: filterStatus || undefined,
					level: filterLevel || undefined,
				});
				issues = res.data;
				totalIssues = res.total;
			} else {
				const res = await api.listIssues(selectedProject, {
					page: currentPage,
					perPage,
					status: filterStatus || undefined,
					level: filterLevel || undefined,
				});
				issues = res.data;
				totalIssues = res.total;
			}
		} catch (e: any) {
			error = e?.message || 'Failed to load issues';
		} finally {
			loading = false;
		}
	}

	function navigate() {
		goto(buildUrl(), { replaceState: true });
		loadIssues();
	}

	function switchProject(slug: string) {
		selectedProject = slug;
		currentPage = 1;
		navigate();
	}

	function setFilter(type: 'status' | 'level', value: string) {
		if (type === 'status') filterStatus = value;
		if (type === 'level') filterLevel = value;
		currentPage = 1;
		navigate();
	}

	function clearFilters() {
		filterStatus = '';
		filterLevel = '';
		searchQuery = '';
		currentPage = 1;
		navigate();
	}

	function goToPage(p: number) {
		if (p < 1 || p > totalPages) return;
		currentPage = p;
		navigate();
	}

	onMount(async () => {
		try {
			projects = await api.listProjects();
			if (projects.length === 0) {
				error = 'No projects found. Create one first.';
				loading = false;
				return;
			}

			// Restore state from URL
			const sp = page.url.searchParams;
			const qp = sp.get('project');
			selectedProject = (qp && projects.some(p => p.slug === qp)) ? qp : projects[0].slug;
			filterStatus = sp.get('status') || '';
			filterLevel = sp.get('level') || '';
			searchQuery = sp.get('q') || '';
			currentPage = parseInt(sp.get('page') || '1', 10) || 1;

			await loadIssues();
		} catch (e: any) {
			error = e?.message || 'Failed to load';
			loading = false;
		}

		// Subscribe to WebSocket for live updates
		const ws = getWsClient();
		ws.connect();
		wsUnsub = ws.subscribe((msg: ServerMessage) => {
			liveIndicator = true;
			setTimeout(() => (liveIndicator = false), 2000);

			if (msg.type === 'IssueUpdated' || msg.type === 'IssueCreated') {
				const incoming = msg.issue as unknown as Issue;
				const idx = issues.findIndex((i) => i.id === incoming.id);
				if (idx >= 0) {
					issues[idx] = incoming;
				} else if (currentPage === 1 && !filterStatus && !filterLevel && !searchQuery) {
					issues.unshift(incoming);
				}
				issues = issues;
			}
		});
	});
</script>

<svelte:head>
	<title>Issues · TrapFall</title>
</svelte:head>

<div class="p-4 lg:p-6 space-y-4">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<h1 class="text-2xl font-bold">Issues</h1>
		<div class="flex items-center gap-2">
			{#if liveIndicator}
				<span class="inline-flex items-center gap-1 text-xs text-emerald-500">
					<span class="h-2 w-2 rounded-full bg-emerald-500 animate-pulse"></span>
					Live
				</span>
			{/if}
			{#if projects.length > 1}
				<select
					class="h-9 rounded-md border border-input bg-background px-3 text-sm"
					bind:value={selectedProject}
					onchange={() => switchProject(selectedProject)}
				>
					{#each projects as p}
						<option value={p.slug}>{p.name}</option>
					{/each}
				</select>
			{:else if projects.length === 1}
				<Badge variant="outline">{projects[0].name}</Badge>
			{/if}
		</div>
	</div>

	<!-- Search + Filters -->
	<div class="flex flex-wrap items-center gap-2">
		<!-- Search input -->
		<input
			type="text"
			value={searchQuery}
			oninput={(e) => {
				searchQuery = (e.target as HTMLInputElement).value;
				clearTimeout(debounceTimer);
				debounceTimer = setTimeout(() => {
					currentPage = 1;
					navigate();
				}, 1500);
			}
			}
			onkeydown={(e) => {
				if (e.key === 'Enter') {
					clearTimeout(debounceTimer);
					currentPage = 1;
					navigate();
				}
			}}
			placeholder="Search... (Enter to search)"
			class="h-8 w-56 rounded-md border border-input bg-background px-3 text-xs placeholder:text-muted-foreground"
		/>

		<!-- Status tabs -->
		<div class="flex rounded-md border overflow-hidden">
			<button
				class="px-3 py-1.5 text-xs font-medium transition-colors {!filterStatus ? 'bg-primary text-primary-foreground' : 'bg-background hover:bg-muted'}"
				onclick={() => setFilter('status', '')}
			>
				All
			</button>
			{#each statuses as s}
				<button
					class="px-3 py-1.5 text-xs font-medium border-l transition-colors {filterStatus === s ? 'bg-primary text-primary-foreground' : 'bg-background hover:bg-muted'}"
					onclick={() => setFilter('status', s)}
				>
					{s}
				</button>
			{/each}
		</div>

		<!-- Level dropdown -->
		<select
			class="h-8 rounded-md border border-input bg-background px-2 text-xs"
			value={filterLevel}
			onchange={(e) => setFilter('level', (e.target as HTMLSelectElement).value)}
		>
			<option value="">All levels</option>
			{#each levels as l}
				<option value={l}>{l}</option>
			{/each}
		</select>

		{#if filterStatus || filterLevel || searchQuery}
			<Button variant="ghost" size="sm" class="h-8 text-xs" onclick={clearFilters}>
				Clear all
			</Button>
		{/if}

		<!-- Count -->
		{#if !loading && totalIssues > 0}
			<span class="text-xs text-muted-foreground ml-auto">
				{totalIssues} issue{totalIssues !== 1 ? 's' : ''}
			</span>
		{/if}
	</div>

	<!-- Content -->
	{#if loading}
		<div class="space-y-3">
			{#each Array(5) as _}
				<Skeleton class="h-12 w-full" />
			{/each}
		</div>
	{:else if error}
		<div class="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
			<p class="text-sm text-destructive">{error}</p>
		</div>
	{:else if issues.length === 0}
		<div class="flex flex-col items-center justify-center py-16 text-center">
			<p class="text-lg font-medium text-muted-foreground">No issues found</p>
			<p class="text-sm text-muted-foreground mt-1">
				{#if searchQuery}
					No results for "{searchQuery}" in {projects.find(p => p.slug === selectedProject)?.name || selectedProject}. Try switching projects.
				{:else if filterStatus || filterLevel}
					Try adjusting your filters.
				{:else}
					Send errors to your DSN and they'll appear here.
				{/if}
			</p>
		</div>
	{:else}
		<div class="rounded-lg border">
			<Table>
				<TableHeader>
					<TableRow>
						<TableHead>Level</TableHead>
						<TableHead class="w-[40%]">Title</TableHead>
						<TableHead>Culprit</TableHead>
						<TableHead>Events</TableHead>
						<TableHead>Users</TableHead>
						<TableHead>Status</TableHead>
						<TableHead>Last Seen</TableHead>
					</TableRow>
				</TableHeader>
				<TableBody>
					{#each issues as issue}
						<TableRow
							class="cursor-pointer hover:bg-muted/50"
							onclick={() => goto(`/issues/${issue.id}?project=${selectedProject}`)}
						>
							<TableCell>
								<Badge variant="outline" class={levelTextClass(issue.level)}>
									{issue.level}
								</Badge>
							</TableCell>
							<TableCell class="font-medium">{issue.title}</TableCell>
							<TableCell class="text-muted-foreground text-sm">
								{issue.culprit || '—'}
							</TableCell>
							<TableCell>{issue.count}</TableCell>
							<TableCell>{issue.user_count}</TableCell>
							<TableCell>
								<Badge variant="outline" class={statusTextClass(issue.status)}>
									{issue.status}
								</Badge>
							</TableCell>
							<TableCell class="text-muted-foreground text-sm">
								{timeAgo(issue.last_seen)}
							</TableCell>
						</TableRow>
					{/each}
				</TableBody>
			</Table>
		</div>

		<!-- Pagination -->
		{#if totalPages > 1}
			<div class="flex items-center justify-between">
				<p class="text-xs text-muted-foreground">
					Showing {(currentPage - 1) * perPage + 1}–{Math.min(currentPage * perPage, totalIssues)} of {totalIssues}
				</p>
				<div class="flex items-center gap-1">
					<Button
						variant="outline"
						size="sm"
						disabled={currentPage <= 1}
						onclick={() => goToPage(currentPage - 1)}
					>
						Prev
					</Button>
					{#each Array(Math.min(totalPages, 5)) as _, i}
						{@const pageNum = currentPage <= 3 ? i + 1 : currentPage - 2 + i}
						{#if pageNum <= totalPages}
							<Button
								variant={pageNum === currentPage ? 'default' : 'outline'}
								size="sm"
								class="w-9"
								onclick={() => goToPage(pageNum)}
							>
								{pageNum}
							</Button>
						{/if}
					{/each}
					<Button
						variant="outline"
						size="sm"
						disabled={currentPage >= totalPages}
						onclick={() => goToPage(currentPage + 1)}
					>
						Next
					</Button>
				</div>
			</div>
		{/if}
	{/if}
</div>
