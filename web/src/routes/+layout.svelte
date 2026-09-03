<script lang="ts">
	import '../app.css';
	import { ModeWatcher } from 'mode-watcher';
	import { Toaster } from '$lib/components/ui/sonner/index.js';
	import { getAuthStore } from '$lib/stores/auth.svelte';
	import { loadTimezone } from '$lib/timezone.svelte';
	import { onMount } from 'svelte';

	let { children }: { children: import('svelte').Snippet } = $props();

	const auth = getAuthStore();

	onMount(() => {
		auth.init();
		loadTimezone();
	});
</script>

<ModeWatcher />
<Toaster position="bottom-right" />

{@render children()}
