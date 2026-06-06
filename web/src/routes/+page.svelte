<script lang="ts">
	import { goto } from '$app/navigation';
	import { getAuthStore } from '$lib/stores/auth';
	import { onMount } from 'svelte';

	const auth = getAuthStore();

	onMount(async () => {
		await auth.init();
		if (auth.isAuthenticated) {
			goto('/issues');
		} else {
			// Check if setup is needed
			try {
				const { api } = await import('$lib/api');
				const status = await api.getSetupStatus();
				if (status.needs_setup) {
					goto('/setup');
				} else {
					goto('/login');
				}
			} catch {
				goto('/login');
			}
		}
	});
</script>

<div class="flex min-h-screen items-center justify-center">
	<p class="text-muted-foreground">Loading...</p>
</div>
