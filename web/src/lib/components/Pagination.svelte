<script lang="ts">
	import { Button } from '$lib/components/ui/button/index.js';

	interface Props {
		page: number;
		totalPages: number;
		total: number;
		perPage: number;
		onPageChange: (page: number) => void;
	}

	let { page, totalPages, total, perPage, onPageChange }: Props = $props();

	function goTo(p: number) {
		if (p < 1 || p > totalPages || p === page) return;
		onPageChange(p);
	}
</script>

{#if totalPages > 1}
	<div class="flex flex-wrap items-center justify-between gap-2">
		<p class="text-xs text-muted-foreground tabular-nums">
			Showing {(page - 1) * perPage + 1}–{Math.min(page * perPage, total)} of {total}
		</p>
		<div class="flex items-center gap-1">
			<Button variant="outline" size="sm" disabled={page <= 1} onclick={() => goTo(page - 1)}>
				Prev
			</Button>
			{#each Array(Math.min(totalPages, 5)) as _, i}
				{@const pageNum = page <= 3 ? i + 1 : page - 2 + i}
				{#if pageNum <= totalPages}
					<Button
						variant={pageNum === page ? 'default' : 'outline'}
						size="sm"
						class="w-9"
						onclick={() => goTo(pageNum)}
					>
						{pageNum}
					</Button>
				{/if}
			{/each}
			<Button variant="outline" size="sm" disabled={page >= totalPages} onclick={() => goTo(page + 1)}>
				Next
			</Button>
		</div>
	</div>
{/if}
