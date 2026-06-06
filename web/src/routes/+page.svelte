<script lang="ts">
	import { goto } from '$app/navigation';
	import { getAuthStore } from '$lib/stores/auth';
	import { onMount } from 'svelte';

	const auth = getAuthStore();

	onMount(async () => {
		await auth.init();
		if (auth.isAuthenticated) {
			goto('/issues');
		}
	});
</script>

{#if auth.loading}
	<div class="flex h-screen items-center justify-center">
		<p class="text-muted-foreground">Loading…</p>
	</div>
{/if}
