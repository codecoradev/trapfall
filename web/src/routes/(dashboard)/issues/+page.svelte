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
	let loading = $state(true);
	let error = $state('');
	let liveIndicator = $state(false);

	import { levelTextClass, statusTextClass, timeAgo } from '$lib/utils';

	let wsUnsub: (() => void) | null = $state(null);

	async function loadIssues() {
		if (!selectedProject) return;
		loading = true;
		error = '';
		try {
			const res = await api.listIssues(selectedProject);
			issues = res.data;
		} catch (e: any) {
			error = e?.message || 'Failed to load issues';
		} finally {
			loading = false;
		}
	}

	onMount(async () => {
		try {
			projects = await api.listProjects();
			if (projects.length === 0) {
				error = 'No projects found. Create one first.';
				loading = false;
				return;
			}

			// Use query param or default to first project
			const queryProject = page.url.searchParams.get('project');
			if (queryProject && projects.some(p => p.slug === queryProject)) {
				selectedProject = queryProject;
			} else {
				selectedProject = projects[0].slug;
			}

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
				} else {
					issues.unshift(incoming);
				}
				issues = issues;
			}
		});
	});

	function switchProject(slug: string) {
		selectedProject = slug;
		goto(`/issues?project=${slug}`, { replaceState: true });
		loadIssues();
	}
</script>

<svelte:head>
	<title>Issues · TrapFall</title>
</svelte:head>

<div class="p-4 lg:p-6 space-y-4">
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
			<p class="text-lg font-medium text-muted-foreground">No issues yet</p>
			<p class="text-sm text-muted-foreground mt-1">
				Send errors to your DSN and they'll appear here.
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
	{/if}
</div>
