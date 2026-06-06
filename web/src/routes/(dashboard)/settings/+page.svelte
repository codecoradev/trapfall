<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type UserInfo } from '$lib/api';
	import { getAuthStore } from '$lib/stores/auth';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '$lib/components/ui/card/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';

	const auth = getAuthStore();
	let user = $derived(auth.user);
	let currentPassword = $state('');
	let newPassword = $state('');
	let confirmPassword = $state('');
	let passwordError = $state('');
	let passwordSuccess = $state('');

	async function changePassword() {
		passwordError = '';
		passwordSuccess = '';

		if (!newPassword || newPassword.length < 8) {
			passwordError = 'Password must be at least 8 characters.';
			return;
		}
		if (newPassword !== confirmPassword) {
			passwordError = 'Passwords do not match.';
			return;
		}

		try {
			await api.post('/auth/change-password', {
				current_password: currentPassword,
				new_password: newPassword
			});
			passwordSuccess = 'Password updated successfully.';
			currentPassword = '';
			newPassword = '';
			confirmPassword = '';
		} catch (e: any) {
			passwordError = e?.message || 'Failed to change password.';
		}
	}
</script>

<svelte:head>
	<title>Settings · TrapFall</title>
</svelte:head>

<div class="p-4 lg:p-6 space-y-6">
	<h1 class="text-2xl font-bold">Settings</h1>

	<!-- Account -->
	<Card>
		<CardHeader>
			<CardTitle>Account</CardTitle>
			<CardDescription>Your account information</CardDescription>
		</CardHeader>
		<CardContent class="space-y-3">
			<div class="grid grid-cols-2 gap-2 text-sm">
				<span class="text-muted-foreground">Name</span>
				<span>{user?.name || '—'}</span>
				<span class="text-muted-foreground">Email</span>
				<span>{user?.email || '—'}</span>
				<span class="text-muted-foreground">Role</span>
				<span class="capitalize">{user?.role || '—'}</span>
			</div>
		</CardContent>
	</Card>

	<!-- Change Password -->
	<Card>
		<CardHeader>
			<CardTitle>Change Password</CardTitle>
		</CardHeader>
		<CardContent>
			<form class="space-y-4 max-w-md" onsubmit={(e) => { e.preventDefault(); changePassword(); }}>
				<div class="space-y-2">
					<Label for="current">Current Password</Label>
					<Input id="current" type="password" bind:value={currentPassword} />
				</div>
				<div class="space-y-2">
					<Label for="new">New Password</Label>
					<Input id="new" type="password" bind:value={newPassword} />
				</div>
				<div class="space-y-2">
					<Label for="confirm">Confirm Password</Label>
					<Input id="confirm" type="password" bind:value={confirmPassword} />
				</div>

				{#if passwordError}
					<p class="text-sm text-destructive">{passwordError}</p>
				{/if}
				{#if passwordSuccess}
					<p class="text-sm text-emerald-500">{passwordSuccess}</p>
				{/if}

				<Button type="submit">Update Password</Button>
			</form>
		</CardContent>
	</Card>
</div>
