<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getAuthStore } from '$lib/stores/auth.svelte';
	import { destroyWsClient } from '$lib/ws';
	import type { UserInfo } from '$lib/api';
	import * as Sheet from '$lib/components/ui/sheet/index.js';

	interface Props {
		title?: string;
		children: import('svelte').Snippet;
	}

	let { title, children }: Props = $props();

	const auth = getAuthStore();

	const navLinks = [
		{ href: '/issues', label: 'Issues' },
		{ href: '/projects', label: 'Projects' },
		{ href: '/rules', label: 'Rules' },
		{ href: '/transactions', label: 'Performance' },
		{ href: '/release-health', label: 'Release Health' },
		{ href: '/settings', label: 'Settings' }
	];

	let mobileMenuOpen = $state(false);

	function isActive(href: string): boolean {
		return page.url.pathname === href || page.url.pathname.startsWith(href + '/');
	}

	function handleLogout() {
		destroyWsClient();
		auth.logout();
		goto('/login');
	}
</script>

<svelte:head>
	<title>{title ? `${title} · ` : ''}TrapFall</title>
</svelte:head>

<div class="min-h-screen bg-background">
	<!-- Top Nav -->
	<header class="sticky top-0 z-40 border-b bg-background">
		<div class="flex h-14 items-center gap-3 px-4 lg:px-6">
			<Sheet.Root bind:open={mobileMenuOpen}>
				<Sheet.Trigger
					class="md:hidden inline-flex items-center justify-center h-9 w-9 rounded-md border border-input bg-background hover:bg-muted transition-colors"
					aria-label="Open menu"
				>
					<svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" x2="20" y1="6" y2="6"/><line x1="4" x2="20" y1="12" y2="12"/><line x1="4" x2="20" y1="18" y2="18"/></svg>
				</Sheet.Trigger>
				<Sheet.Content side="left" class="w-72">
					<Sheet.Header>
						<Sheet.Title>TrapFall</Sheet.Title>
					</Sheet.Header>
					<nav class="flex flex-col px-4 gap-1">
						{#each navLinks as link (link.href)}
							<a
								href={link.href}
								class="rounded-md px-3 py-2.5 text-sm transition-colors {isActive(link.href)
									? 'bg-muted text-foreground font-medium'
									: 'text-muted-foreground hover:bg-muted/50 hover:text-foreground'}"
								onclick={() => (mobileMenuOpen = false)}
							>
								{link.label}
							</a>
						{/each}
					</nav>
				</Sheet.Content>
			</Sheet.Root>
			<a href="/issues" class="font-bold text-lg shrink-0">
				TrapFall
			</a>
			<nav class="hidden md:flex items-center gap-4 text-sm min-w-0">
				{#each navLinks as link (link.href)}
					<a
						href={link.href}
						class="transition-colors {isActive(link.href)
							? 'text-foreground font-medium'
							: 'hover:text-foreground text-muted-foreground'}"
					>
						{link.label}
					</a>
				{/each}
			</nav>
			<div class="ml-auto flex items-center gap-3 shrink-0">
				{#if auth.user}
					<span class="hidden sm:inline text-sm text-muted-foreground max-w-[180px] truncate">
						{auth.user.email}
					</span>
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
