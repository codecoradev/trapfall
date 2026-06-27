<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type Project, type TransactionResponse } from '$lib/api';
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
	import { timeAgo, formatDuration, transactionStatusTextClass } from '$lib/utils';

	let projects: Project[] = $state([]);
	let selectedProject: string = $state('');
	let transactions: TransactionResponse[] = $state([]);
	let slowest: TransactionResponse[] = $state([]);
	let currentPage: number = $state(1);
	let totalTransactions: number = $state(0);
	const perPage = 20;
	let totalPages: number = $derived(Math.max(1, Math.ceil(totalTransactions / perPage)));
	let loading = $state(true);
	let error = $state('');

	async function loadSlowest() {
		if (!selectedProject) return;
		try {
			slowest = await api.getSlowestTransactions(selectedProject, 5);
		} catch {
			slowest = [];
		}
	}

	async function loadTransactions() {
		if (!selectedProject) return;
		loading = true;
		error = '';
		try {
			const res = await api.listTransactions(selectedProject, currentPage, perPage);
			transactions = res.data;
			totalTransactions = res.total;
		} catch (e: any) {
			error = e?.message || 'Failed to load transactions';
		} finally {
			loading = false;
		}
	}

	function buildUrl(): string {
		const params = new URLSearchParams();
		if (selectedProject) params.set('project', selectedProject);
		if (currentPage > 1) params.set('page', String(currentPage));
		return `/transactions?${params.toString()}`;
	}

	function navigate() {
		goto(buildUrl(), { replaceState: true });
		loadTransactions();
	}

	function switchProject(slug: string) {
		selectedProject = slug;
		currentPage = 1;
		navigate();
		loadSlowest();
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

			const sp = page.url.searchParams;
			const qp = sp.get('project');
			selectedProject = (qp && projects.some((p) => p.slug === qp)) ? qp : projects[0].slug;
			currentPage = parseInt(sp.get('page') || '1', 10) || 1;

			await Promise.all([loadSlowest(), loadTransactions()]);
		} catch (e: any) {
			error = e?.message || 'Failed to load';
			loading = false;
		}
	});
</script>

<svelte:head>
	<title>Performance · TrapFall</title>
</svelte:head>

<div class="p-4 lg:p-6 space-y-4">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<h1 class="text-2xl font-bold">Performance</h1>
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

	<!-- Slowest Transactions Widget -->
	{#if slowest.length > 0}
		<Card>
			<CardHeader class="pb-2">
				<CardTitle class="text-lg">Slowest Transactions</CardTitle>
			</CardHeader>
			<CardContent>
				<div class="space-y-2">
					{#each slowest as txn, i}
						<button
							class="w-full flex items-center justify-between rounded-md border px-3 py-2 text-sm text-left hover:bg-muted/50 transition-colors"
							onclick={() => goto(`/transactions/${txn.id}?project=${selectedProject}`)}
						>
							<div class="flex items-center gap-3 min-w-0">
								<span class="text-xs text-muted-foreground font-mono w-4">#{i + 1}</span>
								<span class="font-medium truncate">{txn.name}</span>
							</div>
							<div class="flex items-center gap-3 shrink-0">
								<span class="font-mono text-muted-foreground">{formatDuration(txn.duration_ms)}</span>
								<Badge variant="outline" class={transactionStatusTextClass(txn.status)}>
									{txn.status}
								</Badge>
							</div>
						</button>
					{/each}
				</div>
			</CardContent>
		</Card>
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
	{:else if transactions.length === 0}
		<div class="flex flex-col items-center justify-center py-16 text-center">
			<p class="text-lg font-medium text-muted-foreground">No transactions found</p>
			<p class="text-sm text-muted-foreground mt-1">
				Performance data will appear here once transactions are received.
			</p>
		</div>
	{:else}
		<div class="rounded-lg border">
			<Table>
				<TableHeader>
					<TableRow>
						<TableHead class="w-[40%]">Name</TableHead>
						<TableHead>Duration</TableHead>
						<TableHead>Status</TableHead>
						<TableHead>Release</TableHead>
						<TableHead>Received</TableHead>
					</TableRow>
				</TableHeader>
				<TableBody>
					{#each transactions as txn}
						<TableRow
							class="cursor-pointer hover:bg-muted/50"
							onclick={() => goto(`/transactions/${txn.id}?project=${selectedProject}`)}
						>
							<TableCell class="font-medium">{txn.name}</TableCell>
							<TableCell class="font-mono text-muted-foreground">
								{formatDuration(txn.duration_ms)}
							</TableCell>
							<TableCell>
								<Badge variant="outline" class={transactionStatusTextClass(txn.status)}>
									{txn.status}
								</Badge>
							</TableCell>
							<TableCell class="text-muted-foreground text-sm">
								{txn.release || '—'}
							</TableCell>
							<TableCell class="text-muted-foreground text-sm">
								{timeAgo(txn.received_at)}
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
					Showing {(currentPage - 1) * perPage + 1}–{Math.min(currentPage * perPage, totalTransactions)} of {totalTransactions}
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
