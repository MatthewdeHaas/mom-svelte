<script lang="ts">
  import type { PageData } from './$types';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import Pagination from '$lib/components/Pagination.svelte';

  const { data }: { data: PageData } = $props();

  let search = $state(data.search);
  let debounceTimer: ReturnType<typeof setTimeout>;

  function onSearchInput(e: Event) {
    search = (e.target as HTMLInputElement).value;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => syncParams(), 300);
  }

  function toggleInactive() {
    syncParams(!data.showInactive);
  }

  function syncParams(inactive = data.showInactive) {
    const params = new URLSearchParams($page.url.searchParams);
    if (search) params.set('q', search);
    else params.delete('q');
    if (inactive) params.set('inactive', 'true');
    else params.delete('inactive');
    params.delete('page');
    goto(`?${params.toString()}`, { keepFocus: true, replaceState: true });
  }
</script>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 20px;
    flex-wrap: wrap;
  }
  .search-wrap {
    position: relative;
    flex: 1;
    min-width: 200px;
    max-width: 340px;
  }
  .search-icon {
    position: absolute;
    left: 10px;
    top: 50%;
    transform: translateY(-50%);
    width: 14px;
    height: 14px;
    color: var(--motm-text-muted);
    pointer-events: none;
  }
  .search-input {
    width: 100%;
    padding: 7px 12px 7px 32px;
    border: 1px solid var(--motm-border);
    border-radius: 8px;
    font-size: 13.5px;
    font-family: inherit;
    background: #fff;
    color: var(--motm-text);
    outline: none;
    transition: border-color 120ms;
  }
  .search-input:focus { border-color: var(--motm-pink); }
  .search-input::placeholder { color: var(--motm-text-muted); }

  .toggle-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border: 1px solid var(--motm-border);
    border-radius: 8px;
    font-size: 13px;
    font-family: inherit;
    background: #fff;
    color: var(--motm-text-muted);
    cursor: pointer;
    transition: border-color 120ms, color 120ms, background 120ms;
    white-space: nowrap;
  }
  .toggle-btn:hover { border-color: var(--motm-pink); color: var(--motm-text); }
  .toggle-btn.active {
    background: var(--motm-pink-light);
    border-color: #EAC4D0;
    color: var(--motm-pink-dark);
  }
  .toggle-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
    opacity: 0.5;
  }
  .toggle-btn.active .toggle-dot { opacity: 1; }

  .new-btn {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    background: var(--motm-pink);
    color: #fff;
    border: none;
    cursor: pointer;
    text-decoration: none;
    transition: background 120ms;
  }
  .new-btn:hover { background: var(--motm-pink-dark); }

  .table-wrap {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    overflow: hidden;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  thead tr { border-bottom: 1px solid var(--motm-border); }
  th {
    text-align: left;
    padding: 10px 16px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--motm-text-muted);
    white-space: nowrap;
  }
  td {
    padding: 11px 16px;
    color: var(--motm-text);
    border-bottom: 1px solid var(--motm-border);
    vertical-align: middle;
  }
  tbody tr:last-child td { border-bottom: none; }
  tbody tr:hover td { background: #FAF8F6; }

  .td-muted { color: var(--motm-text-muted); }

  .allergen-pill {
    display: inline-flex;
    padding: 2px 8px;
    border-radius: 20px;
    font-size: 11.5px;
    font-weight: 500;
    background: var(--motm-pink-light);
    color: var(--motm-pink-dark);
    margin: 2px 3px 2px 0;
  }
  .inactive-badge {
    display: inline-flex;
    padding: 2px 7px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    background: #F3F3F1;
    color: #888;
  }
  .result-count {
    font-size: 12.5px;
    color: var(--motm-text-muted);
    margin-bottom: 10px;
  }
  .empty-state {
    padding: 48px;
    text-align: center;
    color: var(--motm-text-muted);
    font-size: 13.5px;
  }
  .empty-title {
    font-weight: 500;
    color: var(--motm-text);
    margin-bottom: 4px;
  }
</style>

<div style="display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 24px;">
  <div>
    <h1 style="font-size: 22px; font-weight: 600; color: var(--motm-text); letter-spacing: -0.02em;">Ingredients</h1>
    <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">Manage ingredients and their allergen flags</p>
  </div>
</div>

<div class="toolbar">
  <div class="search-wrap">
    <svg class="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
    </svg>
    <input
      class="search-input"
      type="search"
      placeholder="Search ingredients…"
      value={search}
      oninput={onSearchInput}
    />
  </div>

  <button
    class="toggle-btn"
    class:active={data.showInactive}
    onclick={toggleInactive}
  >
    <span class="toggle-dot"></span>
    {data.showInactive ? 'Hiding inactive' : 'Show inactive'}
  </button>

  <a href="/admin/ingredients/new" class="new-btn">
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M12 5v14M5 12h14"/>
    </svg>
    New ingredient
  </a>
</div>

{#if data.pagination.total > 0}
  <p class="result-count">{data.pagination.total} {data.pagination.total === 1 ? 'ingredient' : 'ingredients'}</p>
{/if}

<div class="table-wrap">
  {#if data.ingredients.length === 0}
    <div class="empty-state">
      <p class="empty-title">No ingredients found</p>
      <p>{data.search ? 'Try a different search term.' : 'Get started by adding your first ingredient.'}</p>
    </div>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th>Allergens</th>
          <th>Status</th>
        </tr>
      </thead>
      <tbody>
        {#each data.ingredients as ingredient}
          <tr
            onclick={() => goto(`/admin/ingredients/${ingredient.id}`)}
            style="cursor: pointer;"
          >
            <td style="font-weight: 500;">{ingredient.name}</td>
            <td>
              {#if ingredient.allergen?.length}
                {#each ingredient.allergen as allergen}
                  <span class="allergen-pill">{allergen.name}</span>
                {/each}
              {:else}
                <span class="td-muted">—</span>
              {/if}
            </td>
            <td>
              {#if !ingredient.active}
                <span class="inactive-badge">Inactive</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  <Pagination
    page={data.pagination.page}
    totalPages={data.pagination.totalPages}
    total={data.pagination.total}
    perPage={data.pagination.perPage}
  />
</div>
