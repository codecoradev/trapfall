<script lang="ts">
	import { goto } from '$app/navigation';
	import { getAuthStore } from '$lib/stores/auth.svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { onMount } from 'svelte';

	const auth = getAuthStore();

	let email = $state('');
	let password = $state('');
	let error = $state('');
	let loading = $state(false);

	onMount(async () => {
		await auth.init();
		if (auth.isAuthenticated) {
			goto('/');
		}
	});

	async function handleSubmit(e: Event) {
		e.preventDefault();
		error = '';
		loading = true;
		try {
			await auth.login(email, password);
			goto('/');
		} catch (e: any) {
			error = e?.message || 'Login failed';
		} finally {
			loading = false;
		}
	}
</script>

<svelte:head>
	<title>Login · TrapFall</title>
</svelte:head>

<div class="flex min-h-screen items-center justify-center p-4">
	<Card class="w-full max-w-sm">
		<CardHeader class="text-center">
			<CardTitle class="text-2xl">TrapFall</CardTitle>
			<CardDescription>Sign in to your dashboard</CardDescription>
		</CardHeader>
		<CardContent>
			<form onsubmit={handleSubmit} class="space-y-4">
				{#if error}
					<p class="text-sm text-destructive">{error}</p>
				{/if}
				<div class="space-y-2">
					<Label for="email">Email</Label>
					<Input id="email" type="email" bind:value={email} required placeholder="admin@example.com" />
				</div>
				<div class="space-y-2">
					<Label for="password">Password</Label>
					<Input id="password" type="password" bind:value={password} required placeholder="••••••••" />
				</div>
				<Button type="submit" class="w-full" disabled={loading}>
					{loading ? 'Signing in...' : 'Sign in'}
				</Button>
			</form>
		</CardContent>
	</Card>
</div>
