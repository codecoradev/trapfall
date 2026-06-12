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
	let showEditForm: string | null = $state(null);
	let openMenu: string | null = $state(null);
	let tab: 'active' | 'archived' = $state('active');

	let newProjectName = $state('');
	let newProjectSlug = $state('');
	let addError = $state('');
	let addLoading = $state(false);

	let editName = $state('');

	function toggleDsn(id: string) {
		showDsn[id] = !showDsn[id];
	}

	function copyToClipboard(text: string) {
		navigator.clipboard.writeText(text);
	}

	function closeMenuOnOutsideClick(e: MouseEvent) {
		if (openMenu && !(e.target as HTMLElement).closest('.menu-container')) {
			openMenu = null;
		}
	}

	let activeProjects = $derived(projects.filter(p => !p.archived_at));
	let archivedProjects = $derived(projects.filter(p => !!p.archived_at));
	let displayedProjects = $derived(tab === 'active' ? activeProjects : archivedProjects);

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

	async function handleRename(slug: string) {
		if (!editName.trim()) return;
		try {
			await api.updateProject(slug, editName.trim());
			showEditForm = null;
			editName = '';
			openMenu = null;
			await loadProjects();
		} catch (e: any) {
			error = e?.message || 'Failed to rename project';
		}
	}

	async function handleArchive(slug: string) {
		openMenu = null;
		try {
			await api.archiveProject(slug);
			await loadProjects();
		} catch (e: any) {
			error = e?.message || 'Failed to archive project';
		}
	}

	async function handleUnarchive(slug: string) {
		openMenu = null;
		try {
			await api.unarchiveProject(slug);
			await loadProjects();
		} catch (e: any) {
			error = e?.message || 'Failed to unarchive project';
		}
	}

	async function handleDelete(slug: string) {
		if (!confirm('Permanently delete this project and all its data? This cannot be undone.')) return;
		openMenu = null;
		try {
			await api.deleteProject(slug);
			await loadProjects();
		} catch (e: any) {
			error = e?.message || 'Failed to delete project (must be archived first)';
		}
	}

	async function handleRotateDsn(slug: string) {
		openMenu = null;
		try {
			const updated = await api.rotateDsn(slug);
			await loadProjects();
			showDsn[updated.id] = true;
		} catch (e: any) {
			error = e?.message || 'Failed to rotate DSN';
		}
	}

	onMount(loadProjects);
</script>

<svelte:head>
	<title>Projects · TrapFall</title>
</svelte:head>

<svelte:window onclick={closeMenuOnOutsideClick} />

<div class="p-4 lg:p-6 space-y-4">
	<div class="flex items-center justify-between">
		<div class="flex items-center gap-3">
			<h1 class="text-2xl font-bold">Projects</h1>
			<!-- Tab switch -->
			<div class="flex rounded-md border overflow-hidden">
				<button
					class="px-3 py-1.5 text-xs font-medium transition-colors {tab === 'active' ? 'bg-primary text-primary-foreground' : 'bg-background hover:bg-muted'}"
					onclick={() => (tab = 'active')}
				>
					Active ({activeProjects.length})
				</button>
				<button
					class="px-3 py-1.5 text-xs font-medium border-l transition-colors {tab === 'archived' ? 'bg-primary text-primary-foreground' : 'bg-background hover:bg-muted'}"
					onclick={() => (tab = 'archived')}
				>
					Archived ({archivedProjects.length})
				</button>
			</div>
		</div>
		{#if tab === 'active'}
			<Button onclick={() => (showAddForm = !showAddForm)}>
				{showAddForm ? 'Cancel' : '+ Add Project'}
			</Button>
		{/if}
	</div>

	{#if error}
		<div class="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
			<p class="text-sm text-destructive">{error}</p>
		</div>
	{/if}

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
						<Input id="proj-name" type="text" bind:value={newProjectName} required placeholder="My Web App" />
					</div>
					<div class="space-y-2">
						<Label for="proj-slug">Slug (optional, auto-generated from name)</Label>
						<Input id="proj-slug" type="text" bind:value={newProjectSlug} placeholder="my-web-app" />
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
	{:else if displayedProjects.length === 0}
		<div class="flex flex-col items-center justify-center py-16 text-center">
			<p class="text-lg font-medium text-muted-foreground">
				{tab === 'active' ? 'No active projects' : 'No archived projects'}
			</p>
			<p class="text-sm text-muted-foreground mt-1">
				{tab === 'active' ? 'Create a project to start capturing errors.' : 'Archived projects will appear here.'}
			</p>
		</div>
	{:else}
		<div class="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
			{#each displayedProjects as project (project.id)}
				<Card class="relative">
					<CardHeader class="pb-2">
						<div class="flex items-center justify-between">
							<div>
								{#if showEditForm === project.slug}
									<form onsubmit={() => handleRename(project.slug)} class="flex gap-2">
										<Input
											type="text"
											bind:value={editName}
											placeholder={project.name}
											class="h-8 text-sm"
										/>
										<Button type="submit" size="sm" class="h-8">Save</Button>
										<Button type="button" variant="ghost" size="sm" class="h-8" onclick={() => (showEditForm = null)}>X</Button>
									</form>
								{:else}
									<CardTitle class="text-lg">{project.name}</CardTitle>
								{/if}
							</div>
							<div class="flex items-center gap-2">
								<Badge variant="outline">{project.slug}</Badge>
								{#if project.archived_at}
									<Badge variant="secondary">Archived</Badge>
								{/if}
								<!-- Kebab menu -->
								<div class="menu-container relative">
									<Button
										variant="ghost"
										size="sm"
										class="h-8 w-8 p-0"
										onclick={() => {
											openMenu = openMenu === project.id ? null : project.id;
											editName = project.name;
										}}
									>
										⋮
									</Button>
									{#if openMenu === project.id}
										<div class="absolute right-0 top-8 z-10 w-44 rounded-md border bg-background shadow-md">
											{#if !project.archived_at}
												<button
													class="w-full text-left px-3 py-2 text-sm hover:bg-muted"
													onclick={() => { showEditForm = project.slug; openMenu = null; editName = project.name; }}
												>
													Rename
												</button>
												<button
													class="w-full text-left px-3 py-2 text-sm hover:bg-muted"
													onclick={() => handleRotateDsn(project.slug)}
												>
													Rotate DSN
												</button>
												<button
													class="w-full text-left px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
													onclick={() => handleArchive(project.slug)}
												>
													Archive
												</button>
											{:else}
												<button
													class="w-full text-left px-3 py-2 text-sm hover:bg-muted"
													onclick={() => handleUnarchive(project.slug)}
												>
													Unarchive
												</button>
												<button
													class="w-full text-left px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
													onclick={() => handleDelete(project.slug)}
												>
													Delete permanently
												</button>
											{/if}
										</div>
									{/if}
								</div>
							</div>
						</div>
					</CardHeader>
					<CardContent class="space-y-2">
						{#if !project.archived_at}
							<div>
								<p class="text-xs text-muted-foreground mb-1">DSN (use this in your SDK)</p>
								<div class="flex items-center gap-2">
									<code class="text-xs bg-muted px-2 py-1 rounded flex-1 truncate">
										{showDsn[project.id] ? project.dsn : '••••••••••••••••'}
									</code>
									<Button variant="ghost" size="sm" onclick={() => toggleDsn(project.id)}>
										{showDsn[project.id] ? 'Hide' : 'Show'}
									</Button>
									{#if showDsn[project.id]}
										<Button variant="ghost" size="sm" onclick={() => copyToClipboard(project.dsn)}>
											Copy
										</Button>
									{/if}
								</div>
							</div>
						{/if}
						<p class="text-xs text-muted-foreground">
							Created {new Date(project.created_at).toLocaleDateString()}
							{project.archived_at ? ` · Archived ${new Date(project.archived_at).toLocaleDateString()}` : ''}
						</p>
					</CardContent>
				</Card>
			{/each}
		</div>
	{/if}
</div>
