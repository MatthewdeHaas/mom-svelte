<script lang="ts">
    import type { ActionData } from './$types';

    let { form } = $props();
</script>

<div class="min-h-screen bg-gray-50 flex items-center justify-center">
	<div class="bg-white p-8 rounded-lg shadow-sm border border-gray-200 w-full max-w-sm">
		<h1 class="text-xl font-medium text-gray-900 mb-6">Meals on the Move</h1>

		{#if form?.error}
			<p class="text-sm text-red-500 mb-4">{form.error}</p>
		{/if}

		<form method="POST">
			<div class="mb-4">
				<label for="email" class="block text-sm text-gray-700 mb-1">Email</label>
				<input
					id="email"
					name="email"
					type="email"
					required
					class="w-full border border-gray-300 rounded px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-gray-400"
				/>
			</div>
			<div class="mb-6">
				<label for="password" class="block text-sm text-gray-700 mb-1">Password</label>
				<input
					id="password"
					name="password"
					type="password"
					required
					class="w-full border border-gray-300 rounded px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-gray-400"
				/>
			</div>
			<button
				type="submit"
				class="w-full bg-gray-900 text-white rounded px-4 py-2 text-sm hover:bg-gray-700"
			>
				Sign in
			</button>
		</form>
	</div>
</div>
