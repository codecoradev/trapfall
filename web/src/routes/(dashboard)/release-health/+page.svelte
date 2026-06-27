<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type Project, type ReleaseHealthResponse, type CrashRateResponse } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import {
		Table,
		TableBody,
		TableCell,
		TableHead,
		TableHeader,
		TableRow
	} from '$lib/components/ui/table/index.js';
	import { timeAgo, crashRateColor } from '$lib/utils';

	let projects: Project[] = $state([]);
	let selectedProject: string = $state('');
	let sessions: ReleaseHealthResponse[] = $state([]);
	let crashRate: CrashRateResponse | null = $state(null);
	let currentPage: number = $state(1);
	let totalSessions: number = $state(0);
	const perPage = 20;
	let totalPages: number = $derived(Math.max(1, Math.ceil(totalSessions / perPage)));
	let loading = $state(true);
	let error = $state('');
	let filterRelease: string = $state('');
	let filterEnv: string = $state('');
	let debounceTimer: ReturnType<typeof setTimeout> | undefined = undefined;

	const envOptions = [
		{ value: '', label: 'All' },
		{ value: 'production', label: 'Production' },
		{ value: 'development', label: 'Development' }
	];

	let aggregateExited: number = $derived(sessions.reduce((s, r) => s + r.exited, 0));
	let aggregateErrored: number = $derived(sessions.reduce((s, r) => s + r.errored, 0));
	let aggregateAbnormal: number = $derived(sessions.reduce((s, r) => s + r.abnormal, 0));
	let aggregateCrashed: number = $derived(sessions.reduce((s, r) => s + r.crashed, 0));
	let aggregateTotal: number = $derived(aggregateExited + aggregateErrored + aggregateAbnormal + aggregateCrashed);

	async function loadCrashRate() {
		if (!selectedProject) return;
		try {
			crashRate = await api.getCrashRate(
				selectedProject,
				filterRelease || undefined,
				filterEnv || undefined
			);
		} catch {
			crashRate = null;
		}
	}

	async function loadSessions() {
		if (!selectedProject) return;
		loading = true;
		error = '';
		try {
			const res = await api.listReleaseHealth(selectedProject, {
				page: currentPage,
				perPage,
				release: filterRelease || undefined,
				env: filterEnv || undefined
			});
			sessions = res.data;
			totalSessions = res.total;
		} catch (e: any) {
			error = e?.message || 'Failed to load release health data';
		} finally {
			loading = false;
		}
	}

	function buildUrl(): string {
		const params = new URLSearchParams();
		if (selectedProject) params.set('project', selectedProject);
		if (filterRelease) params.set('release', filterRelease);
		if (filterEnv) params.set('env', filterEnv);
		if (currentPage > 1) params.set('page', String(currentPage));
		return `/release-health?${params.toString()}`;
	}

	function navigate() {
		goto(buildUrl(), { replaceState: true });
	}

	function loadData() {
		loadCrashRate();
		loadSessions();
	}

	function switchProject(slug: string) {
		selectedProject = slug;
		currentPage = 1;
		navigate();
		loadData();
	}

	function setEnv(value: string) {
		filterEnv = value;
		currentPage = 1;
		navigate();
		loadData();
	}

	function setRelease(value: string) {
		filterRelease = value;
		clearTimeout(debounceTimer);
		debounceTimer = setTimeout(() => {
			currentPage = 1;
			navigate();
			loadData();
		}, 800);
	}

	function onReleaseKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			clearTimeout(debounceTimer);
			currentPage = 1;
			navigate();
			loadData();
		}
	}

	function goToPage(p: number) {
		if (p < 1 || p > totalPages) return;
		currentPage = p;
		navigate();
		loadSessions();
	}

	onMount(async () => {
		try {
			projects = await api.listProjects();
			if (projects.length === 0) {
				error = 'No projects found. Create one first.';
				loading = false;
				return;
			}

			const sp = page.url.searchParams;
			const qp = sp.get('project');
			selectedProject = (qp && projects.some((p) => p.slug === qp)) ? qp : projects[0].slug;
			filterRelease = sp.get('release') || '';
			filterEnv = sp.get('env') || '';
			currentPage = parseInt(sp.get('page') || '1', 10) || 1;

			await loadData();
		} catch (e: any) {
			error = e?.message || 'Failed to load';
			loading = false;
		}
	});
</script>

<svelte:head>
	<title>Release Health · TrapFall</title>
</svelte:head>

<div class="p-4 lg:p-6 space-y-4">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<h1 class="text-2xl font-bold">Release Health</h1>
		<div class="flex items-center gap-2">
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

	<!-- Filters -->
	<div class="flex flex-wrap items-center gap-2">
		<input
			type="text"
			value={filterRelease}
			oninput={(e) => setRelease((e.target as HTMLInputElement).value)}
			onkeydown={onReleaseKeydown}
			placeholder="Filter by release version..."
			class="h-8 w-56 rounded-md border border-input bg-background px-3 text-xs placeholder:text-muted-foreground"
		/>

		<div class="flex rounded-md border overflow-hidden">
			{#each envOptions as opt}
				<button
					class="px-3 py-1.5 text-xs font-medium transition-colors {filterEnv === opt.value ? 'bg-primary text-primary-foreground' : 'bg-background hover:bg-muted'} {opt.value !== '' ? 'border-l' : ''}"
					onclick={() => setEnv(opt.value)}
				>
					{opt.label}
				</button>
			{/each}
		</div>

		{#if filterRelease || filterEnv}
			<Button
				variant="ghost"
				size="sm"
				class="h-8 text-xs"
				onclick={() => { filterRelease = ''; filterEnv = ''; currentPage = 1; navigate(); loadData(); }}
			>
				Clear all
			</Button>
		{/if}

		{#if !loading && totalSessions > 0}
			<span class="text-xs text-muted-foreground ml-auto">
				{totalSessions} session{totalSessions !== 1 ? 's' : ''}
			</span>
		{/if}
	</div>

	<!-- Crash Rate Card + Session Breakdown -->
	{#if !loading && sessions.length > 0}
		<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
			<Card>
				<CardHeader class="pb-2">
					<CardTitle class="text-lg">Crash Rate</CardTitle>
				</CardHeader>
				<CardContent>
					{#if crashRate !== null}
						<div class="text-4xl font-bold {crashRateColor(crashRate.crash_rate)}">
							{crashRate.crash_rate.toFixed(2)}%
						</div>
						<p class="text-xs text-muted-foreground mt-1">
							{crashRate.crash_rate < 1 ? 'Healthy' : crashRate.crash_rate < 5 ? 'Warning' : 'Critical'}
						</p>
					{:else}
						<div class="text-4xl font-bold text-muted-foreground">—</div>
						<p class="text-xs text-muted-foreground mt-1">No crash rate data</p>
					{/if}
				</CardContent>
			</Card>

			<Card>
				<CardHeader class="pb-2">
					<CardTitle class="text-lg">Session Breakdown</CardTitle>
				</CardHeader>
				<CardContent>
					{#if aggregateTotal > 0}
						<!-- Stacked bar -->
						<div class="flex h-4 rounded-full overflow-hidden bg-muted mb-3">
							{#if aggregateExited > 0}
								<div
									class="bg-green-500 h-full"
									style="width: {(aggregateExited / aggregateTotal * 100)}%"
								></div>
							{/if}
							{#if aggregateErrored > 0}
								<div
									class="bg-yellow-500 h-full"
									style="width: {(aggregateErrored / aggregateTotal * 100)}%"
								></div>
							{/if}
							{#if aggregateAbnormal > 0}
								<div
									class="bg-orange-500 h-full"
									style="width: {(aggregateAbnormal / aggregateTotal * 100)}%"
								></div>
							{/if}
							{#if aggregateCrashed > 0}
								<div
									class="bg-red-500 h-full"
									style="width: {(aggregateCrashed / aggregateTotal * 100)}%"
								></div>
							{/if}
						</div>
						<div class="flex items-center gap-4 text-sm">
							<span class="flex items-center gap-1.5">
								<span class="h-2.5 w-2.5 rounded-full bg-green-500"></span>
								<span class="text-green-600 dark:text-green-400 font-medium">{aggregateExited}</span>
								<span class="text-muted-foreground">exited</span>
							</span>
							<span class="flex items-center gap-1.5">
								<span class="h-2.5 w-2.5 rounded-full bg-yellow-500"></span>
								<span class="text-yellow-600 dark:text-yellow-400 font-medium">{aggregateErrored}</span>
								<span class="text-muted-foreground">errored</span>
							</span>
							<span class="flex items-center gap-1.5">
								<span class="h-2.5 w-2.5 rounded-full bg-orange-500"></span>
								<span class="text-orange-600 dark:text-orange-400 font-medium">{aggregateAbnormal}</span>
								<span class="text-muted-foreground">abnormal</span>
							</span>
							<span class="flex items-center gap-1.5">
								<span class="h-2.5 w-2.5 rounded-full bg-red-500"></span>
								<span class="text-red-600 dark:text-red-400 font-medium">{aggregateCrashed}</span>
								<span class="text-muted-foreground">crashed</span>
							</span>
						</div>
					{:else}
						<p class="text-sm text-muted-foreground">No session data</p>
					{/if}
				</CardContent>
			</Card>
		</div>
	{/if}

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
	{:else if sessions.length === 0}
		<div class="flex flex-col items-center justify-center py-16 text-center">
			<p class="text-lg font-medium text-muted-foreground">No session data yet</p>
			<p class="text-sm text-muted-foreground mt-1">
				Ensure your Sentry SDK has release configured.
			</p>
		</div>
	{:else}
		<div class="rounded-lg border">
			<Table>
				<TableHeader>
					<TableRow>
						<TableHead>Release</TableHead>
						<TableHead>Environment</TableHead>
						<TableHead>Crash Rate</TableHead>
						<TableHead class="text-center">Exited</TableHead>
						<TableHead class="text-center">Errored</TableHead>
						<TableHead class="text-center">Abnormal</TableHead>
						<TableHead class="text-center">Crashed</TableHead>
						<TableHead>Started</TableHead>
					</TableRow>
				</TableHeader>
				<TableBody>
					{#each sessions as s}
						<TableRow>
							<TableCell class="font-medium">{s.release || '—'}</TableCell>
							<TableCell>
								{#if s.environment}
									<Badge variant="outline">{s.environment}</Badge>
								{:else}
									<span class="text-muted-foreground text-sm">—</span>
								{/if}
							</TableCell>
							<TableCell>
								<span class="font-mono font-medium {crashRateColor(s.crash_rate)}">
									{s.crash_rate !== null ? s.crash_rate.toFixed(2) + '%' : '—'}
								</span>
							</TableCell>
							<TableCell class="text-center text-green-600 dark:text-green-400 font-medium">
								{s.exited}
							</TableCell>
							<TableCell class="text-center text-yellow-600 dark:text-yellow-400 font-medium">
								{s.errored}
							</TableCell>
							<TableCell class="text-center text-orange-600 dark:text-orange-400 font-medium">
								{s.abnormal}
							</TableCell>
							<TableCell class="text-center text-red-600 dark:text-red-400 font-medium">
								{s.crashed}
							</TableCell>
							<TableCell class="text-muted-foreground text-sm">
								{timeAgo(s.started_at)}
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
					Showing {(currentPage - 1) * perPage + 1}–{Math.min(currentPage * perPage, totalSessions)} of {totalSessions}
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
