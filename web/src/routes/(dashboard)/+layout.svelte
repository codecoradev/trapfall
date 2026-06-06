<script lang="ts">
	import { goto } from '$app/navigation';
	import { getAuthStore } from '$lib/stores/auth';
	import type { UserInfo } from '$lib/api';

	interface Props {
		title?: string;
		children: import('svelte').Snippet;
	}

	let { title, children }: Props = $props();

	const auth = getAuthStore();

	function handleLogout() {
		auth.logout();
		goto('/login');
	}
</script>

<svelte:head>
	<title>{title ? `${title} · ` : ''}TrapFall</title>
</svelte:head>

<div class="min-h-screen bg-background">
	<!-- Top Nav -->
	<header class="border-b">
		<div class="flex h-14 items-center px-4 lg:px-6">
			<a href="/issues" class="font-bold text-lg mr-6">TrapFall</a>
			<nav class="flex items-center gap-4 text-sm">
				<a href="/issues" class="hover:text-foreground text-muted-foreground transition-colors">
					Issues
				</a>
				<a href="/projects" class="hover:text-foreground text-muted-foreground transition-colors">
					Projects
				</a>
				<a href="/rules" class="hover:text-foreground text-muted-foreground transition-colors">
					Rules
				</a>
				<a href="/settings" class="hover:text-foreground text-muted-foreground transition-colors">
					Settings
				</a>
			</nav>
			<div class="ml-auto flex items-center gap-3">
				{#if auth.user}
					<span class="text-sm text-muted-foreground">{auth.user.email}</span>
					<button
						class="text-sm text-muted-foreground hover:text-foreground transition-colors"
						onclick={handleLogout}
					>
						Log out
					</button>
				{/if}
			</div>
		</div>
	</header>

	<!-- Content -->
	<main>
		{@render children()}
	</main>
</div>
