<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type TransactionDetailResponse, type SpanResponse } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';
	import {
		Table,
		TableBody,
		TableCell,
		TableHead,
		TableHeader,
		TableRow
	} from '$lib/components/ui/table/index.js';
	import { timeAgo, formatDuration, formatTime, transactionStatusTextClass } from '$lib/utils';

	let txn: TransactionDetailResponse | null = $state(null);
	let safeTxn = $derived(txn as unknown as TransactionDetailResponse);
	let loading = $state(true);
	let error = $state('');
	let hoveredSpan: SpanResponse | null = $state(null);

	let projectSlug = $state('');

	function spanColor(op: string | null): string {
		if (!op) return 'bg-gray-400 dark:bg-gray-600';
		const lower = op.toLowerCase();
		if (lower.includes('db') || lower.includes('sql') || lower.includes('postgres') || lower.includes('redis'))
			return 'bg-amber-400 dark:bg-amber-500';
		if (lower.includes('http') || lower.includes('request') || lower.includes('fetch') || lower.includes('client'))
			return 'bg-blue-400 dark:bg-blue-500';
		return 'bg-gray-400 dark:bg-gray-600';
	}

	function spanColorLabel(op: string | null): string {
		if (!op) return 'default';
		const lower = op.toLowerCase();
		if (lower.includes('db') || lower.includes('sql') || lower.includes('postgres') || lower.includes('redis'))
			return 'Database';
		if (lower.includes('http') || lower.includes('request') || lower.includes('fetch') || lower.includes('client'))
			return 'HTTP';
		return 'Other';
	}

	let totalDuration = $derived(safeTxn.duration_ms);

	onMount(async () => {
		const sp = page.url.searchParams;
		projectSlug = sp.get('project') || '';
		const id = page.params.id!;

		if (!projectSlug) {
			error = 'No project specified.';
			loading = false;
			return;
		}

		try {
			txn = await api.getTransaction(projectSlug, id);
		} catch (e: any) {
			error = e?.message || 'Failed to load transaction';
		} finally {
			loading = false;
		}
	});
</script>

<svelte:head>
	<title>{txn ? `${txn.name} · Performance` : 'Transaction · Performance'} · TrapFall</title>
</svelte:head>

<div class="p-4 lg:p-6 space-y-4">
	<!-- Back button -->
	<Button variant="ghost" size="sm" class="mb-2" onclick={() => goto(`/transactions?project=${projectSlug}`)}>
		← Back to Performance
	</Button>

	{#if loading}
		<div class="space-y-3">
			<Skeleton class="h-8 w-64" />
			<Skeleton class="h-48 w-full" />
			<Skeleton class="h-32 w-full" />
		</div>
	{:else if error}
		<div class="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
			<p class="text-sm text-destructive">{error}</p>
		</div>
	{:else if txn}
		<!-- Header -->
		<Card>
			<CardHeader>
				<div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2">
					<div>
						<CardTitle class="text-xl">{txn.name}</CardTitle>
						<p class="text-sm text-muted-foreground mt-1">
							ID: <span class="font-mono text-xs">{txn.id}</span>
						</p>
					</div>
					<div class="flex items-center gap-2 flex-wrap">
						{#if txn.release}
							<Badge variant="outline">{txn.release}</Badge>
						{/if}
						{#if txn.environment}
							<Badge variant="secondary">{txn.environment}</Badge>
						{/if}
						<Badge variant="outline" class={transactionStatusTextClass(txn.status)}>
							{txn.status}
						</Badge>
						<span class="font-mono text-lg font-bold">{formatDuration(txn.duration_ms)}</span>
					</div>
				</div>
			</CardHeader>
			<CardContent>
				<p class="text-xs text-muted-foreground">Received {formatTime(txn.received_at)} ({timeAgo(txn.received_at)})</p>
			</CardContent>
		</Card>

		<!-- Waterfall Chart -->
		{#if txn.spans.length > 0}
			<Card>
				<CardHeader class="pb-2">
					<div class="flex items-center justify-between">
						<CardTitle class="text-lg">Span Waterfall</CardTitle>
						<div class="flex items-center gap-3 text-xs text-muted-foreground">
							<span class="flex items-center gap-1">
								<span class="h-3 w-3 rounded bg-amber-400 dark:bg-amber-500 inline-block"></span>
								Database
							</span>
							<span class="flex items-center gap-1">
								<span class="h-3 w-3 rounded bg-blue-400 dark:bg-blue-500 inline-block"></span>
								HTTP
							</span>
							<span class="flex items-center gap-1">
								<span class="h-3 w-3 rounded bg-gray-400 dark:bg-gray-600 inline-block"></span>
								Other
							</span>
						</div>
					</div>
				</CardHeader>
				<CardContent>
					<div class="relative overflow-x-auto">
						<div class="min-w-[600px]">
							<!-- Time axis labels -->
							<div class="flex mb-1 ml-[180px] mr-4 relative">
								{#each [0, 25, 50, 75, 100] as pct}
									<span class="absolute text-[10px] text-muted-foreground" style="left: {pct}%; transform: translateX(-50%)">
										{formatDuration((pct / 100) * totalDuration)}
									</span>
								{/each}
							</div>

							<!-- Spans -->
							{#each txn.spans as span}
								{@const leftPct = (span.start_offset_ms / totalDuration) * 100}
								{@const widthPct = Math.max(0.3, (span.duration_ms / totalDuration) * 100)}
								<div
									class="flex items-center h-8 group"
									onmouseenter={() => (hoveredSpan = span)}
									onmouseleave={() => (hoveredSpan = null)}
								>
									<!-- Span label -->
									<div class="w-[180px] shrink-0 pr-3 text-xs text-muted-foreground truncate text-right" title={span.op || ''}>
										{span.op || 'unknown'}
									</div>
									<!-- Bar container -->
									<div class="flex-1 relative h-6 mr-4">
										<div
											class="absolute top-0 h-full rounded-sm {spanColor(span.op)} opacity-80 group-hover:opacity-100 transition-opacity cursor-pointer"
											style="left: {leftPct}%; width: {widthPct}%; min-width: 3px;"
											title="{span.op}: {span.description || ''} ({formatDuration(span.duration_ms)})"
										></div>
									</div>
								</div>
							{/each}

							<!-- Hovered span detail -->
							{#if hoveredSpan}
								<div class="mt-2 ml-[180px] text-xs text-muted-foreground bg-muted rounded-md px-3 py-2 inline-block">
									<strong>{hoveredSpan.op || 'unknown'}</strong>
									{#if hoveredSpan.description}
										— {hoveredSpan.description}
									{/if}
									<span class="ml-2 font-mono">{formatDuration(hoveredSpan.duration_ms)}</span>
									<span class="ml-2">offset: {formatDuration(hoveredSpan.start_offset_ms)}</span>
									{#if hoveredSpan.status}
										<span class="ml-2">({hoveredSpan.status})</span>
									{/if}
								</div>
							{/if}
						</div>
					</div>
				</CardContent>
			</Card>

			<Separator class="my-4" />

			<!-- Span Table -->
			<Card>
				<CardHeader class="pb-2">
					<CardTitle class="text-lg">Spans ({txn.spans.length})</CardTitle>
				</CardHeader>
				<CardContent>
					<div class="rounded-lg border overflow-x-auto">
						<Table>
							<TableHeader>
								<TableRow>
									<TableHead>Operation</TableHead>
									<TableHead>Description</TableHead>
									<TableHead>Start</TableHead>
									<TableHead>Duration</TableHead>
									<TableHead>Status</TableHead>
								</TableRow>
							</TableHeader>
							<TableBody>
								{#each txn.spans as span}
									<TableRow>
										<TableCell>
											<div class="flex items-center gap-2">
												<span class="h-2.5 w-2.5 rounded-sm shrink-0 {spanColor(span.op)}"></span>
												<span class="font-medium font-mono text-xs">{span.op || '—'}</span>
											</div>
										</TableCell>
										<TableCell class="text-muted-foreground text-sm max-w-[300px] truncate">
											{span.description || '—'}
										</TableCell>
										<TableCell class="font-mono text-muted-foreground text-xs">
											+{formatDuration(span.start_offset_ms)}
										</TableCell>
										<TableCell class="font-mono">
											{formatDuration(span.duration_ms)}
										</TableCell>
										<TableCell>
											{#if span.status}
												<Badge variant="outline" class={transactionStatusTextClass(span.status)}>
													{span.status}
												</Badge>
											{:else}
												<span class="text-muted-foreground">—</span>
											{/if}
										</TableCell>
									</TableRow>
								{/each}
							</TableBody>
						</Table>
					</div>
				</CardContent>
			</Card>
		{:else}
			<Card>
				<CardContent class="py-8 text-center text-muted-foreground">
					No spans recorded for this transaction.
				</CardContent>
			</Card>
		{/if}
	{/if}
</div>
