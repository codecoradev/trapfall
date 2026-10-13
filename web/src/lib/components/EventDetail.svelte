<script lang="ts">
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { levelTextClass } from '$lib/utils';
	import {
		formatValue,
		frameLocation,
		groupFrames,
		parseBreadcrumbs,
		parseContexts,
		parseExceptions,
		parseExtra,
		parseRequest,
		parseSummary,
		parseTags,
		parseUser,
		isDict,
		type KeyValue
	} from '$lib/event-detail';

	let { data }: { data: Record<string, unknown> | null | undefined } = $props();

	const exceptions = $derived(parseExceptions(data));
	const breadcrumbs = $derived(parseBreadcrumbs(data));
	const tags = $derived(parseTags(data));
	const request = $derived(parseRequest(data));
	const user = $derived(parseUser(data));
	const contexts = $derived(parseContexts(data));
	const summary = $derived(parseSummary(data));
	const extra = $derived(parseExtra(data));
	const message = $derived(typeof data?.message === 'string' ? data.message : '');
	const raw = $derived(JSON.stringify(data ?? null, null, 2));

	let tab: 'details' | 'raw' = $state('details');
	// Keys of expanded library-frame groups, e.g. "0-2".
	let expanded: Record<string, boolean> = $state({});
	let copied = $state(false);

	function toggle(key: string) {
		expanded[key] = !expanded[key];
	}

	function onTabKey(e: KeyboardEvent) {
		if (e.key === 'ArrowRight' || e.key === 'ArrowLeft' || e.key === 'Home' || e.key === 'End') {
			e.preventDefault();
			tab = tab === 'details' ? 'raw' : 'details';
			document.getElementById(`event-tab-${tab}`)?.focus();
		}
	}

	async function copyRaw() {
		try {
			await navigator.clipboard.writeText(raw);
			copied = true;
			setTimeout(() => (copied = false), 1500);
		} catch {
			copied = false;
		}
	}

	function time(ts: string | null): string {
		if (!ts) return '';
		const d = new Date(ts);
		return Number.isNaN(d.getTime()) ? ts : d.toISOString().slice(11, 23);
	}

	const hasAnything = $derived(
		exceptions.length + breadcrumbs.length + tags.length + user.length + contexts.length + summary.length + extra.length > 0 ||
			request !== null ||
			message !== ''
	);
</script>

{#snippet kvTable(rows: KeyValue[], label: string)}
	<dl class="grid grid-cols-[minmax(6rem,12rem)_1fr] gap-x-4 gap-y-1 text-sm" aria-label={label}>
		{#each rows as row (row.key)}
			<dt class="text-muted-foreground truncate" title={row.key}>{row.key}</dt>
			<dd class="font-mono text-xs break-all pt-0.5">{row.value}</dd>
		{/each}
	</dl>
{/snippet}

<div class="space-y-3">
	<div role="tablist" aria-label="Event view" class="inline-flex rounded-md border p-0.5 text-sm">
		{#each [['details', 'Details'], ['raw', 'Raw JSON']] as [id, label] (id)}
			<button
				type="button"
				role="tab"
				id="event-tab-{id}"
				aria-selected={tab === id}
				aria-controls="event-panel-{id}"
				tabindex={tab === id ? 0 : -1}
				onkeydown={onTabKey}
				onclick={() => (tab = id as 'details' | 'raw')}
				class="rounded px-3 py-1 focus-visible:outline-2 focus-visible:outline-ring {tab === id
					? 'bg-primary text-primary-foreground'
					: 'text-muted-foreground hover:text-foreground'}"
			>
				{label}
			</button>
		{/each}
	</div>

	{#if tab === 'details'}
		<div role="tabpanel" id="event-panel-details" aria-labelledby="event-tab-details" class="space-y-4">
			{#if !hasAnything}
				<p class="text-sm text-muted-foreground">
					This event has no structured data. Check the Raw JSON tab.
				</p>
			{/if}

			{#if message}
				<Card>
					<CardHeader class="pb-2"><CardTitle class="text-sm">Message</CardTitle></CardHeader>
					<CardContent><p class="text-sm break-words whitespace-pre-wrap">{message}</p></CardContent>
				</Card>
			{/if}

			{#each exceptions as exc, ei (ei)}
				<Card>
					<CardHeader class="pb-2">
						<CardTitle class="text-sm font-mono break-all">
							{exc.type}{#if exc.value}<span class="font-normal text-muted-foreground">: {exc.value}</span>{/if}
						</CardTitle>
					</CardHeader>
					<CardContent>
						{#if exc.frames.length === 0}
							<p class="text-sm text-muted-foreground">No stack trace available.</p>
						{:else}
							<ol class="space-y-1" aria-label="Stack trace, most recent call first">
								{#each groupFrames(exc.frames) as group, gi (gi)}
									{@const key = `${ei}-${gi}`}
									{#if group.collapsed && !expanded[key]}
										<li>
											<Button
												variant="ghost"
												size="sm"
												class="h-7 text-xs text-muted-foreground"
												aria-expanded="false"
												onclick={() => toggle(key)}
											>
												Show {group.frames.length} library frame{group.frames.length === 1 ? '' : 's'}
											</Button>
										</li>
									{:else}
										{#if group.collapsed}
											<li>
												<Button
													variant="ghost"
													size="sm"
													class="h-7 text-xs text-muted-foreground"
													aria-expanded="true"
													onclick={() => toggle(key)}
												>
													Hide {group.frames.length} library frame{group.frames.length === 1 ? '' : 's'}
												</Button>
											</li>
										{/if}
										{#each group.frames as f, fi (fi)}
											<li
												class="rounded border-l-2 px-3 py-2 {f.inApp
													? 'border-l-primary bg-primary/5'
													: 'border-l-transparent bg-muted/40 text-muted-foreground'}"
											>
												<div class="flex flex-wrap items-center gap-2 text-xs font-mono break-all">
													<span class={f.inApp ? 'font-semibold text-foreground' : ''}>{frameLocation(f)}</span>
													{#if f.function}<span>in <span class="text-foreground">{f.function}</span></span>{/if}
													{#if f.inApp}<Badge variant="outline" class="text-[10px]">in app</Badge>{/if}
												</div>
												{#if f.contextLine !== null}
													<pre
														class="mt-2 overflow-x-auto text-xs leading-5"
														aria-label="Source context">{#each f.preContext as l}<span class="block opacity-60">{l}</span>{/each}<span class="block bg-destructive/15 text-foreground">{f.contextLine}</span>{#each f.postContext as l}<span class="block opacity-60">{l}</span>{/each}</pre>
												{/if}
											</li>
										{/each}
									{/if}
								{/each}
							</ol>
						{/if}
					</CardContent>
				</Card>
			{/each}

			{#if breadcrumbs.length > 0}
				<Card>
					<CardHeader class="pb-2"><CardTitle class="text-sm">Breadcrumbs ({breadcrumbs.length})</CardTitle></CardHeader>
					<CardContent>
						<ol class="relative space-y-3 border-l pl-4" aria-label="Breadcrumbs, oldest first">
							{#each breadcrumbs as b, i (i)}
								<li class="relative">
									<span class="absolute -left-[1.3rem] top-1.5 size-2 rounded-full bg-muted-foreground" aria-hidden="true"></span>
									<div class="flex flex-wrap items-center gap-2 text-xs">
										{#if b.timestamp}<time class="font-mono text-muted-foreground" datetime={b.timestamp}>{time(b.timestamp)}</time>{/if}
										<Badge variant="outline" class="text-[10px]">{b.category || b.type}</Badge>
										<span class={levelTextClass(b.level)}>{b.level}</span>
									</div>
									{#if b.message}<p class="text-sm break-words">{b.message}</p>{/if}
									{#if isDict(b.data) && Object.keys(b.data).length > 0}
										<p class="text-xs font-mono text-muted-foreground break-all">{formatValue(b.data)}</p>
									{/if}
								</li>
							{/each}
						</ol>
					</CardContent>
				</Card>
			{/if}

			{#if tags.length > 0}
				<Card>
					<CardHeader class="pb-2"><CardTitle class="text-sm">Tags</CardTitle></CardHeader>
					<CardContent>
						<ul class="flex flex-wrap gap-2" aria-label="Tags">
							{#each tags as t (t.key)}
								<li class="rounded-full border px-2.5 py-0.5 text-xs">
									<span class="text-muted-foreground">{t.key}</span>
									<span class="font-mono ml-1">{t.value}</span>
								</li>
							{/each}
						</ul>
					</CardContent>
				</Card>
			{/if}

			{#if request}
				<Card>
					<CardHeader class="pb-2"><CardTitle class="text-sm">Request</CardTitle></CardHeader>
					<CardContent class="space-y-3">
						{#if request.url || request.method}
							<p class="text-sm font-mono break-all">
								{#if request.method}<Badge variant="secondary" class="mr-2">{request.method}</Badge>{/if}{request.url}{#if request.query}?{request.query}{/if}
							</p>
						{/if}
						{#if request.headers.length > 0}
							<h3 class="text-xs font-medium text-muted-foreground">Headers</h3>
							{@render kvTable(request.headers, 'Request headers')}
						{/if}
						{#if request.rows.length > 0}
							{@render kvTable(request.rows, 'Request details')}
						{/if}
					</CardContent>
				</Card>
			{/if}

			{#if user.length > 0}
				<Card>
					<CardHeader class="pb-2"><CardTitle class="text-sm">User</CardTitle></CardHeader>
					<CardContent>{@render kvTable(user, 'User')}</CardContent>
				</Card>
			{/if}

			{#if summary.length > 0 || contexts.length > 0}
				<Card>
					<CardHeader class="pb-2"><CardTitle class="text-sm">Environment</CardTitle></CardHeader>
					<CardContent class="space-y-4">
						{#if summary.length > 0}{@render kvTable(summary, 'Release and environment')}{/if}
						<div class="grid gap-4 md:grid-cols-2">
							{#each contexts as c (c.name)}
								<section aria-label="{c.name} context">
									<h3 class="mb-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">{c.name}</h3>
									{@render kvTable(c.rows, `${c.name} context`)}
								</section>
							{/each}
						</div>
					</CardContent>
				</Card>
			{/if}

			{#if extra.length > 0}
				<Card>
					<CardHeader class="pb-2"><CardTitle class="text-sm">Additional data</CardTitle></CardHeader>
					<CardContent>{@render kvTable(extra, 'Additional data')}</CardContent>
				</Card>
			{/if}
		</div>
	{:else}
		<div role="tabpanel" id="event-panel-raw" aria-labelledby="event-tab-raw">
			<Card>
				<CardHeader class="pb-2 flex-row items-center justify-between">
					<CardTitle class="text-sm font-mono">Raw JSON</CardTitle>
					<Button variant="outline" size="sm" onclick={copyRaw} aria-live="polite">{copied ? 'Copied' : 'Copy'}</Button>
				</CardHeader>
				<CardContent>
					<!-- Scrollable region must be keyboard focusable -->
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
					<pre
						class="text-xs bg-muted p-3 rounded overflow-auto max-h-[32rem]"
						tabindex="0"
						aria-label="Raw event JSON">{raw}</pre>
				</CardContent>
			</Card>
		</div>
	{/if}
</div>
