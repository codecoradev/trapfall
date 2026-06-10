<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type Project } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';

	let projects: Project[] = $state([]);
	let loading = $state(true);
	let error = $state('');
	let showDsn: Record<string, boolean> = $state({});
	let showAddForm = $state(false);
	let newProjectName = $state('');
	let newProjectSlug = $state('');
	let addError = $state('');
	let addLoading = $state(false);

	function toggleDsn(id: string) {
		showDsn[id] = !showDsn[id];
	}

	function copyToClipboard(text: string) {
		navigator.clipboard.writeText(text);
	}

	async function loadProjects() {
		try {
			projects = await api.listProjects();
		} catch (e: any) {
			error = e?.message || 'Failed to load projects';
		} finally {
			loading = false;
		}
	}

	async function handleAddProject(e: Event) {
		e.preventDefault();
		addError = '';
		addLoading = true;
		try {
			const slug = newProjectSlug || undefined;
			await api.createProject(newProjectName, slug);
			newProjectName = '';
			newProjectSlug = '';
			showAddForm = false;
			await loadProjects();
		} catch (e: any) {
			addError = e?.message || 'Failed to create project';
		} finally {
			addLoading = false;
		}
	}

	onMount(loadProjects);
</script>

<svelte:head>
	<title>Projects · TrapFall</title>
</svelte:head>

<div class="p-4 lg:p-6 space-y-4">
	<div class="flex items-center justify-between">
		<h1 class="text-2xl font-bold">Projects</h1>
		<Button onclick={() => (showAddForm = !showAddForm)}>
			{showAddForm ? 'Cancel' : '+ Add Project'}
		</Button>
	</div>

	{#if showAddForm}
		<Card>
			<CardHeader>
				<CardTitle class="text-lg">New Project</CardTitle>
			</CardHeader>
			<CardContent>
				<form onsubmit={handleAddProject} class="space-y-4 max-w-md">
					{#if addError}
						<p class="text-sm text-destructive">{addError}</p>
					{/if}
					<div class="space-y-2">
						<Label for="proj-name">Project Name</Label>
						<Input
							id="proj-name"
							type="text"
							bind:value={newProjectName}
							required
							placeholder="My Web App"
						/>
					</div>
					<div class="space-y-2">
						<Label for="proj-slug">Slug (optional, auto-generated from name)</Label>
						<Input
							id="proj-slug"
							type="text"
							bind:value={newProjectSlug}
							placeholder="my-web-app"
						/>
					</div>
					<Button type="submit" disabled={addLoading}>
						{addLoading ? 'Creating...' : 'Create Project'}
					</Button>
				</form>
			</CardContent>
		</Card>
	{/if}

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
							<p class="text-xs text-muted-foreground mb-1">DSN (use this in your SDK)</p>
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
