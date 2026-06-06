<script lang="ts">
	import { goto } from '$app/navigation';
	import { getAuthStore } from '$lib/stores/auth';
	import { api } from '$lib/api';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';

	const auth = getAuthStore();

	let email = $state('');
	let name = $state('');
	let password = $state('');
	let confirmPassword = $state('');
	let error = $state('');
	let loading = $state(false);
	let dsn = $state('');

	async function handleSubmit(e: Event) {
		e.preventDefault();
		error = '';

		if (password !== confirmPassword) {
			error = 'Passwords do not match';
			return;
		}

		loading = true;
		try {
			const res = await auth.setup(email, name, password);
			dsn = res.dsn;
		} catch (e: any) {
			error = e?.message || 'Setup failed';
		} finally {
			loading = false;
		}
	}

	function handleContinue() {
		goto('/');
	}
</script>

<svelte:head>
	<title>Setup · TrapFall</title>
</svelte:head>

<div class="flex min-h-screen items-center justify-center p-4">
	<Card class="w-full max-w-md">
		<CardHeader class="text-center">
			<CardTitle class="text-2xl">Welcome to TrapFall</CardTitle>
			<CardDescription>Create your admin account to get started</CardDescription>
		</CardHeader>
		<CardContent>
			{#if dsn}
				<div class="space-y-4">
					<p class="text-sm text-muted-foreground">Your admin account has been created. Here's your default project DSN:</p>
					<div class="rounded-lg border bg-muted p-3">
						<code class="text-xs break-all">{dsn}</code>
					</div>
					<Button onclick={handleContinue} class="w-full">Go to Dashboard</Button>
				</div>
			{:else}
				<form onsubmit={handleSubmit} class="space-y-4">
					{#if error}
						<p class="text-sm text-destructive">{error}</p>
					{/if}
					<div class="space-y-2">
						<Label for="name">Name</Label>
						<Input id="name" type="text" bind:value={name} required placeholder="Admin" />
					</div>
					<div class="space-y-2">
						<Label for="email">Email</Label>
						<Input id="email" type="email" bind:value={email} required placeholder="admin@example.com" />
					</div>
					<div class="space-y-2">
						<Label for="password">Password</Label>
						<Input id="password" type="password" bind:value={password} required placeholder="••••••••" />
					</div>
					<div class="space-y-2">
						<Label for="confirm">Confirm Password</Label>
						<Input id="confirm" type="password" bind:value={confirmPassword} required placeholder="••••••••" />
					</div>
					<Button type="submit" class="w-full" disabled={loading}>
						{loading ? 'Creating...' : 'Create Account'}
					</Button>
				</form>
			{/if}
		</CardContent>
	</Card>
</div>
