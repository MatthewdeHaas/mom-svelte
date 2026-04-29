<script lang="ts">
  import type { PageData, ActionData } from './$types';
  import { enhance } from '$app/forms';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  // Seed selected allergens from the ingredient's current allergens,
  // or from form repopulation on validation error
  const initialIds = form?.values?.allergenIds
    ?? data.ingredient.allergens?.map((a: { id: string }) => a.id)
    ?? [];

  let selected = $state<Set<string>>(new Set(initialIds));

  function toggleAllergen(id: string) {
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
    selected = new Set(selected);
  }

  // Re-sync when the page data updates after a successful action
  $effect(() => {
    if (form?.action === 'update' && form?.success) {
      // selection already reflects what was submitted — no reset needed
    }
  });
</script>

<style>
  .back-link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 13px;
    color: var(--motm-text-muted);
    text-decoration: none;
    margin-bottom: 20px;
    transition: color 120ms;
  }
  .back-link:hover { color: var(--motm-text); }

  .page-title {
    font-size: 22px;
    font-weight: 600;
    color: var(--motm-text);
    letter-spacing: -0.02em;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .inactive-chip {
    font-size: 12px;
    font-weight: 600;
    padding: 3px 9px;
    border-radius: 5px;
    background: #F3F3F1;
    color: #888;
  }

  .card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    max-width: 560px;
    margin-bottom: 20px;
  }
  .card-header {
    padding: 18px 22px 14px;
    border-bottom: 1px solid var(--motm-border);
  }
  .card-title { font-size: 14px; font-weight: 600; color: var(--motm-text); }
  .card-body {
    padding: 22px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .card-footer {
    padding: 14px 22px;
    border-top: 1px solid var(--motm-border);
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    align-items: center;
  }

  .field { display: flex; flex-direction: column; gap: 5px; }
  label { font-size: 12.5px; font-weight: 600; color: var(--motm-text); }
  .field-hint { font-size: 12px; color: var(--motm-text-muted); }
  .field-error { font-size: 12px; color: #A32D2D; }

  input[type="text"] {
    padding: 8px 11px;
    border: 1px solid var(--motm-border);
    border-radius: 7px;
    font-size: 13.5px;
    font-family: inherit;
    color: var(--motm-text);
    background: #fff;
    outline: none;
    transition: border-color 120ms;
  }
  input[type="text"]:focus { border-color: var(--motm-pink); }
  input[type="text"].error { border-color: #E24B4A; }
  input[type="text"]:disabled {
    background: #FAF8F6;
    color: var(--motm-text-muted);
    cursor: not-allowed;
  }

  .pill-group {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding-top: 2px;
  }
  .pill-toggle {
    padding: 5px 13px;
    border-radius: 20px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: 1.5px solid var(--motm-border);
    background: #fff;
    color: var(--motm-text-muted);
    transition: border-color 120ms, background 120ms, color 120ms;
    user-select: none;
  }
  .pill-toggle:hover:not(:disabled) {
    border-color: var(--motm-pink);
    color: var(--motm-text);
  }
  .pill-toggle.selected {
    background: var(--motm-pink-light);
    border-color: var(--motm-pink);
    color: var(--motm-pink-dark);
  }
  .pill-toggle:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .success-strip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: #D1FAE5;
    color: #1F6648;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 500;
    margin-bottom: 16px;
    max-width: 560px;
  }

  .btn {
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms, color 120ms;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    text-decoration: none;
  }
  .btn-primary { background: var(--motm-pink); color: #fff; }
  .btn-primary:hover { background: var(--motm-pink-dark); }
  .btn-ghost {
    background: transparent;
    color: var(--motm-text-muted);
    border: 1px solid var(--motm-border);
  }
  .btn-ghost:hover { background: #FAF8F6; color: var(--motm-text); }
  .btn-danger {
    background: transparent;
    color: #A32D2D;
    border: 1px solid #F09595;
  }
  .btn-danger:hover { background: #FCEBEB; }

  .danger-zone { border-color: #F0CBCB; max-width: 560px; }
  .danger-zone .card-header { border-bottom-color: #F0CBCB; }
  .danger-zone-body {
    padding: 20px 22px;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 24px;
    flex-wrap: wrap;
  }
  .danger-desc {
    font-size: 13px;
    color: var(--motm-text-muted);
    max-width: 340px;
    line-height: 1.55;
  }
</style>

<a href="/admin/ingredients" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Ingredients
</a>

<div style="margin-bottom: 24px;">
  <h1 class="page-title">
    {data.ingredient.name}
    {#if !data.ingredient.active}
      <span class="inactive-chip">Inactive</span>
    {/if}
  </h1>
  <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">Edit ingredient details and allergen flags</p>
</div>

{#if form?.action === 'update' && form?.success}
  <div class="success-strip">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Changes saved
  </div>
{/if}
{#if form?.action === 'reactivate' && form?.success}
  <div class="success-strip">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Ingredient reactivated — you can now edit it
  </div>
{/if}
{#if form?.action === 'deactivate' && form?.success}
  <div class="success-strip" style="background: #F3F3F1; color: #555;">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Ingredient deactivated
  </div>
{/if}

<div class="card">
  <div class="card-header">
    <p class="card-title">Ingredient details</p>
  </div>

  <form method="POST" action="?/update" use:enhance>
    <!-- Hidden inputs for selected allergens -->
    {#each [...selected] as id}
      <input type="hidden" name="allergen_ids" value={id} />
    {/each}

    <div class="card-body">
      <div class="field">
        <label for="name">Name</label>
        <input
          id="name"
          name="name"
          type="text"
          class:error={form?.errors?.name}
          value={form?.values?.name ?? data.ingredient.name}
          disabled={!data.ingredient.active}
          required
        />
        {#if form?.errors?.name}
          <span class="field-error">{form.errors.name}</span>
        {/if}
      </div>

      <div class="field">
        <label>Allergens</label>
        <p class="field-hint">Select all that apply</p>
        {#if data.allergens.length === 0}
          <p style="font-size: 13px; color: var(--motm-text-muted); font-style: italic;">No allergens configured.</p>
        {:else}
          <div class="pill-group">
            {#each data.allergens as allergen}
              <button
                type="button"
                class="pill-toggle"
                class:selected={selected.has(allergen.id)}
                disabled={!data.ingredient.active}
                onclick={() => toggleAllergen(allergen.id)}
              >
                {allergen.name}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>

    <div class="card-footer">
      {#if !data.ingredient.active}
        <span style="font-size: 12.5px; color: var(--motm-text-muted);">Inactive ingredients cannot be edited</span>
        <form method="POST" action="?/reactivate" use:enhance>
          <button type="submit" class="btn btn-primary">Reactivate ingredient</button>
        </form>
      {:else}
        <button type="submit" class="btn btn-primary">Save changes</button>
      {/if}
    </div>
  </form>
</div>

<!-- Danger zone — only shown when active -->
{#if data.ingredient.active}
  <div class="card danger-zone">
    <div class="card-header">
      <p class="card-title" style="color: #A32D2D;">Danger zone</p>
    </div>
    <div class="danger-zone-body">
      <p class="danger-desc">
        Deactivating this ingredient removes it from the active catalogue. Any recipes that already use it are unaffected — this is non-destructive.
      </p>
      <form method="POST" action="?/deactivate" use:enhance>
        <button type="submit" class="btn btn-danger">Deactivate ingredient</button>
      </form>
    </div>
  </div>
{/if}
