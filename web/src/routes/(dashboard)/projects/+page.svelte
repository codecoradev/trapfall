<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type Project } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';

	let projects: Project[] = $state([]);
	let loading = $state(true);
	let error = $state('');
	let showDsn: Record<string, boolean> = $state({});

	function toggleDsn(id: string) {
		showDsn[id] = !showDsn[id];
	}

	function copyToClipboard(text: string) {
		navigator.clipboard.writeText(text);
	}

	onMount(async () => {
		try {
			projects = await api.listProjects();
		} catch (e: any) {
			error = e?.message || 'Failed to load projects';
		} finally {
			loading = false;
		}
	});
</script>

<svelte:head>
	<title>Projects · TrapFall</title>
</svelte:head>

<div class="p-4 lg:p-6 space-y-4">
	<div class="flex items-center justify-between">
		<h1 class="text-2xl font-bold">Projects</h1>
	</div>

	{#if loading}
		<div class="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
			{#each Array(3) as _}
				<Skeleton class="h-32 w-full" />
			{/each}
		</div>
	{:else if error}
		<div class="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
			<p class="text-sm text-destructive">{error}</p>
		</div>
	{:else if projects.length === 0}
		<div class="flex flex-col items-center justify-center py-16 text-center">
			<p class="text-lg font-medium text-muted-foreground">No projects</p>
			<p class="text-sm text-muted-foreground mt-1">Create a project to start capturing errors.</p>
		</div>
	{:else}
		<div class="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
			{#each projects as project}
				<Card>
					<CardHeader class="pb-2">
						<div class="flex items-center justify-between">
							<CardTitle class="text-lg">{project.name}</CardTitle>
							<Badge variant="outline">{project.slug}</Badge>
						</div>
					</CardHeader>
					<CardContent class="space-y-2">
						<div>
							<p class="text-xs text-muted-foreground mb-1">DSN</p>
							<div class="flex items-center gap-2">
								<code class="text-xs bg-muted px-2 py-1 rounded flex-1 truncate">
									{showDsn[project.id] ? project.dsn : '••••••••••••••••'}
								</code>
								<Button
									variant="ghost"
									size="sm"
									onclick={() => toggleDsn(project.id)}
								>
									{showDsn[project.id] ? 'Hide' : 'Show'}
								</Button>
								{#if showDsn[project.id]}
									<Button
										variant="ghost"
										size="sm"
										onclick={() => copyToClipboard(project.dsn)}
									>
										Copy
									</Button>
								{/if}
							</div>
						</div>
						<p class="text-xs text-muted-foreground">
							Created {new Date(project.created_at).toLocaleDateString()}
						</p>
					</CardContent>
				</Card>
			{/each}
		</div>
	{/if}
</div>
