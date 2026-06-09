<script lang="ts">
	import { goto } from '$app/navigation';
	import { getAuthStore } from '$lib/stores/auth.svelte';
	import { api } from '$lib/api';
	import { onMount } from 'svelte';

	const auth = getAuthStore();

	onMount(async () => {
		try {
			const status = await api.getSetupStatus();
			if (status.needs_setup) {
				goto('/setup');
				return;
			}
		} catch {
			// If setup status fails, continue to auth check
		}

		await auth.init();
		if (auth.isAuthenticated) {
			goto('/issues');
		} else {
			goto('/login');
		}
	});
</script>

{#if auth.loading}
	<div class="flex h-screen items-center justify-center">
		<p class="text-muted-foreground">Loading…</p>
	</div>
{/if}
