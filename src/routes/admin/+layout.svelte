<script lang="ts">
    import type { LayoutData } from './$types';

    let { data, children } = $props();
</script>

<div class="min-h-screen bg-gray-50">
    <nav class="bg-white border-b border-gray-200 px-6 py-4 flex items-center justify-between">
        <span class="font-medium text-gray-900">Meals on the Move</span>
        <div class="flex items-center gap-6 text-sm text-gray-600">
            <a href="/admin/dashboard">Dashboard</a>
            <a href="/admin/orders">Orders</a>
            <a href="/admin/companies">Companies</a>
            <a href="/admin/menus">Menus</a>
            <a href="/admin/recipes">Recipes</a>
            <a href="/admin/exports">Exports</a>
            <span class="text-gray-400">|</span>
            <span>{data.user.email}</span>
            <form method="POST" action="/logout">
                <button type="submit" class="text-red-500 hover:text-red-700">Sign out</button>
            </form>
        </div>
    </nav>

    <main class="p-6">
        {@render children()}
    </main>
</div>
