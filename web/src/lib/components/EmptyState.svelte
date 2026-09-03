<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		description?: string;
		/** Optional action (e.g. a Button) rendered below the description. */
		children?: Snippet;
	}

	let { title, description, children }: Props = $props();
</script>

<div class="flex flex-col items-center justify-center py-16 text-center">
	<h3 class="text-lg font-medium text-foreground">{title}</h3>
	{#if description}
		<p class="mt-1 max-w-md text-sm text-muted-foreground">{description}</p>
	{/if}
	{#if children}
		<div class="mt-4">{@render children()}</div>
	{/if}
</div>
