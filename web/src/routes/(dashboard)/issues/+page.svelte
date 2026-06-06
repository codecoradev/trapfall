<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
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
	let project: Project | null = $state(null);
	let loading = $state(true);
	let error = $state('');
	let liveIndicator = $state(false);

	const levelColors: Record<string, string> = {
		fatal: 'bg-red-500/15 text-red-500',
		error: 'bg-orange-500/15 text-orange-500',
		warning: 'bg-yellow-500/15 text-yellow-500',
		info: 'bg-blue-500/15 text-blue-500',
		debug: 'bg-gray-500/15 text-gray-400',
		trace: 'bg-gray-500/15 text-gray-500'
	};

	const statusColors: Record<string, string> = {
		unresolved: 'bg-red-500/15 text-red-400',
		resolved: 'bg-emerald-500/15 text-emerald-400',
		ignored: 'bg-gray-500/15 text-gray-400',
		regression: 'bg-orange-500/15 text-orange-400'
	};

	function timeAgo(dateStr: string): string {
		const d = new Date(dateStr);
		const now = new Date();
		const diff = Math.floor((now.getTime() - d.getTime()) / 1000);
		if (diff < 60) return `${diff}s ago`;
		if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
		if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
		return `${Math.floor(diff / 86400)}d ago`;
	}

	let wsUnsub: (() => void) | null = $state(null);

	onMount(async () => {
		try {
			const projects = await api.listProjects();
			if (projects.length === 0) {
				error = 'No projects found. Create one via setup.';
				loading = false;
				return;
			}
			project = projects[0];

			const res = await api.listIssues(project.slug);
			issues = res.data;
		} catch (e: any) {
			error = e?.message || 'Failed to load issues';
		} finally {
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
			{#if project}
				<Badge variant="outline">{project.name}</Badge>
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
							onclick={() => goto(`/issues/${issue.id}`)}
						>
							<TableCell>
								<Badge variant="outline" class={levelColors[issue.level] || ''}>
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
								<Badge variant="outline" class={statusColors[issue.status] || ''}>
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
