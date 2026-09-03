<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type Project, type AlertRule, type CreateAlertRule } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card } from '$lib/components/ui/card/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';

	let rules: AlertRule[] = $state([]);
	let projects: Project[] = $state([]);
	let selectedProject: string = $state('');
	let loading = $state(true);
	let error = $state('');
	/** Rule id awaiting delete confirmation in the AlertDialog. */
	let pendingDelete: AlertRule | null = $state(null);
	let showForm = $state(false);

	let formName = $state('');
	let formLevel = $state('error');
	let formCountGte = $state('');
	let formWebhookUrl = $state('');
	let formCooldown = $state('300');

	async function loadRules() {
		if (!selectedProject) return;
		loading = true;
		error = '';
		try {
			rules = await api.listAlertRules(selectedProject);
		} catch (e: any) {
			error = e?.message || 'Failed to load rules';
		} finally {
			loading = false;
		}
	}

	onMount(async () => {
		try {
			projects = await api.listProjects();
			if (projects.length === 0) {
				error = 'No projects found.';
				loading = false;
				return;
			}

			// Restore from URL or default to first
			const queryProject = page.url.searchParams.get('project');
			if (queryProject && projects.some(p => p.slug === queryProject)) {
				selectedProject = queryProject;
			} else {
				selectedProject = projects[0].slug;
			}

			await loadRules();
		} catch (e: any) {
			error = e?.message || 'Failed to load';
			loading = false;
		}
	});

	function switchProject(slug: string) {
		selectedProject = slug;
		goto(`/rules?project=${slug}`, { replaceState: true });
		loadRules();
	}

	async function handleCreate() {
		if (!selectedProject || !formName.trim()) return;

		const conditions: Record<string, unknown> = {};
		if (formLevel) conditions.level = [formLevel];
		if (formCountGte && parseInt(formCountGte) > 0) conditions.count_gte = parseInt(formCountGte);

		const actionConfig: Record<string, unknown> = {};
		if (formWebhookUrl.trim()) actionConfig.url = formWebhookUrl.trim();

		try {
			await api.createAlertRule(selectedProject, {
				name: formName.trim(),
				conditions,
				action_type: 'webhook',
				action_config: actionConfig,
				cooldown_seconds: parseInt(formCooldown) || 300
			});
			showForm = false;
			formName = '';
			formWebhookUrl = '';
			toast.success('Rule created');
			await loadRules();
		} catch (e: any) {
			toast.error(e?.message || 'Failed to create rule');
		}
	}

	async function handleToggle(rule: AlertRule) {
		try {
			await api.toggleAlertRule(rule.id, !rule.enabled);
			toast.success(rule.enabled ? 'Rule disabled' : 'Rule enabled');
			await loadRules();
		} catch (e: any) {
			toast.error(e?.message || 'Failed to toggle rule');
		}
	}

	async function handleDelete() {
		if (!pendingDelete) return;
		const id = pendingDelete.id;
		pendingDelete = null;
		try {
			await api.deleteAlertRule(id);
			toast.success('Rule deleted');
			await loadRules();
		} catch (e: any) {
			toast.error(e?.message || 'Failed to delete rule');
		}
	}
</script>

<svelte:head>
	<title>Alert Rules · TrapFall</title>
</svelte:head>

<div class="p-4 lg:p-6 space-y-4">
	<div class="flex flex-wrap items-center justify-between gap-2">
		<h1 class="text-2xl font-bold whitespace-nowrap">Alert Rules</h1>
		<div class="flex items-center gap-2">
			{#if projects.length > 1}
				<select
					class="h-9 min-w-0 max-w-[190px] rounded-md border border-input bg-background px-3 text-sm"
					value={selectedProject}
					onchange={() => switchProject((event?.target as HTMLSelectElement).value)}
				>
					{#each projects as p}
						<option value={p.slug}>{p.name}</option>
					{/each}
				</select>
			{/if}
			<Button onclick={() => (showForm = !showForm)} variant={showForm ? 'secondary' : 'default'}>
				{showForm ? 'Cancel' : '+ New Rule'}
			</Button>
		</div>
	</div>

	{#if error}
		<div class="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
			<p class="text-sm text-destructive">{error}</p>
		</div>
	{/if}

	{#if showForm}
		<Card class="p-4 space-y-4">
			<h2 class="text-lg font-semibold">Create Alert Rule</h2>
			<Separator />

			<div class="grid gap-4 md:grid-cols-2">
				<div class="space-y-2">
					<Label for="name">Rule Name</Label>
					<Input id="name" bind:value={formName} placeholder="e.g. High error rate" />
				</div>
				<div class="space-y-2">
					<Label for="level">Trigger Level</Label>
					<select
						id="level"
						bind:value={formLevel}
						class="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm"
					>
						<option value="error">Error</option>
						<option value="fatal">Fatal</option>
						<option value="warning">Warning</option>
						<option value="info">Info</option>
					</select>
				</div>
				<div class="space-y-2">
					<Label for="count">Min Event Count</Label>
					<Input id="count" bind:value={formCountGte} placeholder="e.g. 10" type="number" />
				</div>
				<div class="space-y-2">
					<Label for="cooldown">Cooldown (seconds)</Label>
					<Input id="cooldown" bind:value={formCooldown} placeholder="300" type="number" />
				</div>
			</div>

			<div class="space-y-2">
				<Label for="webhook">Webhook URL</Label>
				<Input id="webhook" bind:value={formWebhookUrl} placeholder="https://hooks.example.com/..." />
			</div>

			<Button onclick={handleCreate} disabled={!formName.trim()}>Create Rule</Button>
		</Card>
	{/if}

	{#if loading}
		<p class="text-muted-foreground">Loading rules...</p>
	{:else if rules.length === 0 && !showForm}
		<EmptyState
			title="No alert rules yet"
			description="Create a rule to get notified when errors match your conditions."
		/>
	{:else}
		<div class="space-y-3">
			{#each rules as rule}
				<Card class="p-4">
					<div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
						<div class="min-w-0 space-y-1">
							<div class="flex flex-wrap items-center gap-2">
								<h3 class="font-medium break-words">{rule.name}</h3>
								<Badge variant={rule.enabled ? 'default' : 'secondary'}>
									{rule.enabled ? 'Active' : 'Disabled'}
								</Badge>
								<Badge variant="outline">{rule.action_type}</Badge>
							</div>
							<p class="text-sm text-muted-foreground break-words">
								Level: {JSON.stringify(rule.conditions?.level ?? 'any')}
								{rule.conditions?.count_gte ? ` | Count ≥ ${rule.conditions.count_gte}` : ''}
								{rule.conditions?.title_contains
									? ` | Title contains "${rule.conditions.title_contains}"`
									: ''}
							</p>
							<p class="text-xs text-muted-foreground break-all">
								Cooldown: {rule.cooldown_seconds}s ·
								{rule.action_config?.url || 'no webhook configured'}
							</p>
						</div>
						<div class="flex gap-2 shrink-0">
							<Button variant="outline" size="sm" onclick={() => handleToggle(rule)}>
								{rule.enabled ? 'Disable' : 'Enable'}
							</Button>
							<Button variant="destructive" size="sm" onclick={() => (pendingDelete = rule)}>
								Delete
							</Button>
						</div>
					</div>
				</Card>
			{/each}
		</div>
	{/if}

	<!-- Confirm dialog for rule deletion -->
	<AlertDialog.Root open={pendingDelete !== null} onOpenChange={(o) => (pendingDelete = o ? pendingDelete : null)}>
		<AlertDialog.Content>
			<AlertDialog.Header>
				<AlertDialog.Title>Delete rule "{pendingDelete?.name}"?</AlertDialog.Title>
				<AlertDialog.Description>
					Alerts matching this rule will no longer be sent. This cannot be undone.
				</AlertDialog.Description>
			</AlertDialog.Header>
			<AlertDialog.Footer>
				<AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
				<AlertDialog.Action
					class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
					onclick={handleDelete}
				>
					Delete rule
				</AlertDialog.Action>
			</AlertDialog.Footer>
		</AlertDialog.Content>
	</AlertDialog.Root>
</div>
