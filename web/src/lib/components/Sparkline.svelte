<script lang="ts">
	interface Props {
		/** Buckets to plot; zero values still render as a visible empty slot. */
		values: number[];
		/** Accessible description; screen readers read this instead of the bars. */
		label: string;
	}

	let { values, label }: Props = $props();

	let max = $derived(Math.max(...values, 0));

	function barPct(v: number): number {
		if (max <= 0 || v <= 0) return 2;
		return Math.max(3, (v / max) * 100);
	}
</script>

{#if values.length > 0}
	<div class="flex items-end gap-0.5 h-12 w-full" role="img" aria-label={label}>
		{#each values as v, i (i)}
			<div
				class="flex-1 rounded-[2px] {v === 0 ? 'bg-muted-foreground/25' : 'bg-primary'}"
				style="height: {barPct(v)}%"
			></div>
		{/each}
	</div>
{/if}
