<script lang="ts">
  import type { PageData, ActionData } from './$types';
  import { enhance } from '$app/forms';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  // Common units for the add-row selector
  const COMMON_UNITS = ['g', 'kg', 'ml', 'L', 'tsp', 'tbsp', 'cup', 'oz', 'lb', 'piece', 'slice', 'bunch', 'pinch'];

  // Reset add-row fields after successful add
  let addAmount = $state('');
  let addUnit = $state('');
  let addIngredientId = $state('');

  $effect(() => {
    if (form?.action === 'addIngredient' && form?.success) {
      addAmount = '';
      addUnit = '';
      addIngredientId = '';
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
    flex-wrap: wrap;
  }
  .inactive-chip {
    font-size: 12px;
    font-weight: 600;
    padding: 3px 9px;
    border-radius: 5px;
    background: #F3F3F1;
    color: #888;
  }

  /* Cards */
  .card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    margin-bottom: 20px;
  }
  .card-header {
    padding: 18px 22px 14px;
    border-bottom: 1px solid var(--motm-border);
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .card-title { font-size: 14px; font-weight: 600; color: var(--motm-text); }
  .card-sub { font-size: 12.5px; color: var(--motm-text-muted); margin-top: 2px; }
  .card-body { padding: 22px; }
  .card-footer {
    padding: 14px 22px;
    border-top: 1px solid var(--motm-border);
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    align-items: center;
  }

  /* Name form */
  .name-field { display: flex; gap: 10px; align-items: flex-start; }
  .name-field input {
    flex: 1;
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
  .name-field input:focus { border-color: var(--motm-pink); }
  .name-field input:disabled { background: #FAF8F6; color: var(--motm-text-muted); cursor: not-allowed; }
  .name-field input.error { border-color: #E24B4A; }
  .field-error { font-size: 12px; color: #A32D2D; margin-top: 4px; }

  /* Ingredients table */
  .ing-table { width: 100%; border-collapse: collapse; font-size: 13px; }
  .ing-table th {
    text-align: left;
    padding: 9px 12px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--motm-text-muted);
    border-bottom: 1px solid var(--motm-border);
  }
  .ing-table td {
    padding: 11px 12px;
    border-bottom: 1px solid var(--motm-border);
    vertical-align: middle;
    color: var(--motm-text);
  }
  .ing-table tbody tr:last-child td { border-bottom: none; }
  .ing-table .td-muted { color: var(--motm-text-muted); }

  /* Add row — sits inside the table as the final row */
  .add-row td { background: #FAF8F6; padding: 10px 12px; border-top: 1.5px dashed var(--motm-border); border-bottom: none; }
  .add-row select,
  .add-row input {
    padding: 6px 9px;
    border: 1px solid var(--motm-border);
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    color: var(--motm-text);
    background: #fff;
    outline: none;
    transition: border-color 120ms;
    width: 100%;
  }
  .add-row select:focus,
  .add-row input:focus { border-color: var(--motm-pink); }
  .add-row select.error,
  .add-row input.error { border-color: #E24B4A; }
  .add-row-error { font-size: 11.5px; color: #A32D2D; margin-top: 3px; }

  .empty-ingredients {
    padding: 28px;
    text-align: center;
    color: var(--motm-text-muted);
    font-size: 13px;
    border-bottom: 1px solid var(--motm-border);
  }

  /* Buttons */
  .btn {
    padding: 7px 14px;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms, color 120ms;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
    text-decoration: none;
  }
  .btn-primary { background: var(--motm-pink); color: #fff; }
  .btn-primary:hover { background: var(--motm-pink-dark); }
  .btn-sm { padding: 5px 10px; font-size: 12px; }
  .btn-ghost {
    background: transparent;
    color: var(--motm-text-muted);
    border: 1px solid var(--motm-border);
  }
  .btn-ghost:hover { background: #FAF8F6; color: var(--motm-text); }
  .btn-remove {
    background: transparent;
    color: #A32D2D;
    border: 1px solid transparent;
    padding: 4px 8px;
  }
  .btn-remove:hover { background: #FCEBEB; border-color: #F09595; border-radius: 6px; }
  .btn-danger {
    background: transparent;
    color: #A32D2D;
    border: 1px solid #F09595;
  }
  .btn-danger:hover { background: #FCEBEB; }

  /* Success / status strips */
  .strip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 500;
    margin-bottom: 16px;
  }
  .strip-success { background: #D1FAE5; color: #1F6648; }
  .strip-neutral { background: #F3F3F1; color: #555; }

  /* Soft-delete notice */
  .soft-delete-notice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 14px 16px;
    background: #FEF9EC;
    border: 1px solid #F5D97A;
    border-radius: 8px;
    font-size: 13px;
    color: #7A5800;
    line-height: 1.55;
    margin-bottom: 20px;
  }
  .soft-delete-notice svg { flex-shrink: 0; margin-top: 1px; }

  /* Danger zone */
  .danger-zone { border-color: #F0CBCB; }
  .danger-zone .card-header { border-bottom-color: #F0CBCB; }
  .danger-zone-body {
    padding: 20px 22px;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 24px;
    flex-wrap: wrap;
  }
  .danger-desc { font-size: 13px; color: var(--motm-text-muted); max-width: 380px; line-height: 1.55; }
</style>

<a href="/admin/recipes" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Recipes
</a>

<div style="margin-bottom: 24px;">
  <h1 class="page-title">
    {data.recipe.name}
    {#if !data.recipe.active}
      <span class="inactive-chip">Inactive</span>
    {/if}
  </h1>
  <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">
    {data.recipe.ingredients?.length ?? 0} ingredient{(data.recipe.ingredients?.length ?? 0) === 1 ? '' : 's'}
  </p>
</div>

<!-- Status strips -->
{#if form?.action === 'updateName' && form?.success}
  <div class="strip strip-success">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Name updated
  </div>
{/if}
{#if form?.action === 'reactivate' && form?.success}
  <div class="strip strip-success">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Recipe reactivated
  </div>
{/if}
{#if form?.action === 'deactivate' && form?.success}
  <div class="strip strip-neutral">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Recipe deactivated
  </div>
{/if}

<!-- Soft-delete awareness notice for inactive recipes -->
{#if !data.recipe.active}
  <div class="soft-delete-notice">
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="12" cy="12" r="10"/><path d="M12 8v4m0 4h.01"/>
    </svg>
    This recipe is inactive and will not appear in menus or new orders. All historical orders referencing it are preserved and remain fully accessible.
  </div>
{/if}

<!-- Name card -->
<div class="card">
  <div class="card-header">
    <div>
      <p class="card-title">Recipe name</p>
    </div>
  </div>
  <form method="POST" action="?/updateName" use:enhance>
    <div class="card-body">
      <div class="name-field">
        <input
          name="name"
          type="text"
          class:error={form?.action === 'updateName' && form?.errors?.name}
          value={form?.action === 'updateName' ? (form?.values?.name ?? data.recipe.name) : data.recipe.name}
          disabled={!data.recipe.active}
          required
        />
        {#if data.recipe.active}
          <button type="submit" class="btn btn-primary btn-sm" style="margin-top: 1px;">Save</button>
        {/if}
      </div>
      {#if form?.action === 'updateName' && form?.errors?.name}
        <p class="field-error">{form.errors.name}</p>
      {/if}
    </div>
    {#if !data.recipe.active}
      <div class="card-footer">
        <span style="font-size: 12.5px; color: var(--motm-text-muted);">Inactive recipes cannot be edited</span>
        <form method="POST" action="?/reactivate" use:enhance>
          <button type="submit" class="btn btn-primary btn-sm">Reactivate recipe</button>
        </form>
      </div>
    {/if}
  </form>
</div>

<!-- Ingredients card -->
<div class="card">
  <div class="card-header">
    <div>
      <p class="card-title">Ingredients</p>
      <p class="card-sub">Changes apply immediately</p>
    </div>
  </div>

  <table class="ing-table">
    <thead>
      <tr>
        <th>Ingredient</th>
        <th>Amount</th>
        <th>Unit</th>
        {#if data.recipe.active}<th></th>{/if}
      </tr>
    </thead>
    <tbody>
      {#if !data.recipe.ingredients?.length}
        <tr>
          <td colspan="4" class="empty-ingredients">
            No ingredients yet — add the first one below
          </td>
        </tr>
      {:else}
        {#each data.recipe.ingredients as ing}
          <tr>
            <td style="font-weight: 500;">{ing.ingredient_name ?? ing.name}</td>
            <td class="td-muted">{ing.amount}</td>
            <td class="td-muted">{ing.unit}</td>
            {#if data.recipe.active}
              <td style="text-align: right;">
                <form method="POST" action="?/removeIngredient" use:enhance>
                  <input type="hidden" name="ingredient_id" value={ing.ingredient_id} />
                  <button type="submit" class="btn btn-remove" title="Remove">
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                      <path d="M18 6 6 18M6 6l12 12"/>
                    </svg>
                  </button>
                </form>
              </td>
            {/if}
          </tr>
        {/each}
      {/if}

      <!-- Add row — only shown when recipe is active and there are ingredients left to add -->
      {#if data.recipe.active && data.availableIngredients.length > 0}
        <tr class="add-row">
          <td>
            <form
              id="add-ingredient-form"
              method="POST"
              action="?/addIngredient"
              use:enhance
            >
              <select
                name="ingredient_id"
                class:error={form?.action === 'addIngredient' && form?.errors?.ingredient_id}
                bind:value={addIngredientId}
              >
                <option value="">Select ingredient…</option>
                {#each data.availableIngredients as ing}
                  <option value={ing.id}>{ing.name}</option>
                {/each}
              </select>
              {#if form?.action === 'addIngredient' && form?.errors?.ingredient_id}
                <p class="add-row-error">{form.errors.ingredient_id}</p>
              {/if}
            </form>
          </td>
          <td>
            <input
              form="add-ingredient-form"
              name="amount"
              type="number"
              min="0.01"
              step="any"
              placeholder="0"
              class:error={form?.action === 'addIngredient' && form?.errors?.amount}
              bind:value={addAmount}
            />
            {#if form?.action === 'addIngredient' && form?.errors?.amount}
              <p class="add-row-error">{form.errors.amount}</p>
            {/if}
          </td>
          <td>
            <select
              form="add-ingredient-form"
              name="unit"
              class:error={form?.action === 'addIngredient' && form?.errors?.unit}
              bind:value={addUnit}
            >
              <option value="">Unit…</option>
              {#each COMMON_UNITS as unit}
                <option value={unit}>{unit}</option>
              {/each}
            </select>
            {#if form?.action === 'addIngredient' && form?.errors?.unit}
              <p class="add-row-error">{form.errors.unit}</p>
            {/if}
          </td>
          <td style="text-align: right;">
            <button form="add-ingredient-form" type="submit" class="btn btn-primary btn-sm">
              Add
            </button>
          </td>
        </tr>
      {:else if data.recipe.active && data.availableIngredients.length === 0 && (data.recipe.ingredients?.length ?? 0) > 0}
        <tr>
          <td colspan="4" style="padding: 12px; text-align: center; font-size: 12.5px; color: var(--motm-text-muted);">
            All available ingredients have been added
          </td>
        </tr>
      {/if}
    </tbody>
  </table>
</div>

<!-- Danger zone — active recipes only -->
{#if data.recipe.active}
  <div class="card danger-zone">
    <div class="card-header">
      <p class="card-title" style="color: #A32D2D;">Danger zone</p>
    </div>
    <div class="danger-zone-body">
      <p class="danger-desc">
        Deactivating this recipe removes it from menus and new orders. This is non-destructive — the recipe and all its order history are permanently preserved and remain fully accessible to administrators.
      </p>
      <form method="POST" action="?/deactivate" use:enhance>
        <button type="submit" class="btn btn-danger">Deactivate recipe</button>
      </form>
    </div>
  </div>
{/if}
