<script lang="ts">
  import type { PageData, ActionData } from './$types';
  import { enhance } from '$app/forms';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  // Initialise selected allergens from form repopulation on validation error
  let selected = $state<Set<string>>(new Set());

  $effect(() => { selected = new Set(form?.values?.allergenIds ?? []); });

  function toggleAllergen(id: string) {
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
    selected = new Set(selected); // trigger reactivity
  }
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

  .card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    max-width: 560px;
  }
  .card-header {
    padding: 20px 24px 16px;
    border-bottom: 1px solid var(--motm-border);
  }
  .card-title { font-size: 15px; font-weight: 600; color: var(--motm-text); }
  .card-body {
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .card-footer {
    padding: 16px 24px;
    border-top: 1px solid var(--motm-border);
    display: flex;
    gap: 10px;
    justify-content: flex-end;
  }

  .field { display: flex; flex-direction: column; gap: 5px; }
  label, .field-label { font-size: 12.5px; font-weight: 600; color: var(--motm-text); }
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

  /* Pill toggles */
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
  .pill-toggle:hover {
    border-color: var(--motm-pink);
    color: var(--motm-text);
  }
  .pill-toggle.selected {
    background: var(--motm-pink-light);
    border-color: var(--motm-pink);
    color: var(--motm-pink-dark);
  }
  .no-allergens {
    font-size: 13px;
    color: var(--motm-text-muted);
    font-style: italic;
  }

  .btn {
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms;
    text-decoration: none;
    display: inline-flex;
    align-items: center;
  }
  .btn-primary { background: var(--motm-pink); color: #fff; }
  .btn-primary:hover { background: var(--motm-pink-dark); }
  .btn-ghost {
    background: transparent;
    color: var(--motm-text-muted);
    border: 1px solid var(--motm-border);
  }
  .btn-ghost:hover { background: #FAF8F6; color: var(--motm-text); }
</style>

<a href="/admin/ingredients" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Ingredients
</a>

<div style="margin-bottom: 24px;">
  <h1 style="font-size: 22px; font-weight: 600; color: var(--motm-text); letter-spacing: -0.02em;">New ingredient</h1>
  <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">Add a new ingredient to the catalogue</p>
</div>

<div class="card">
  <div class="card-header">
    <p class="card-title">Ingredient details</p>
  </div>

  <form method="POST" use:enhance>
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
          value={form?.values?.name ?? ''}
          placeholder="e.g. Whole milk"
          required
        />
        {#if form?.errors?.name}
          <span class="field-error">{form.errors.name}</span>
        {/if}
      </div>

      <div class="field">
        <span class="field-label">Allergens</span>
        <p class="field-hint">Select all that apply</p>
        {#if data.allergens.length === 0}
          <p class="no-allergens">No allergens configured yet.</p>
        {:else}
          <div class="pill-group">
            {#each data.allergens as allergen}
              <button
                type="button"
                class="pill-toggle"
                class:selected={selected.has(allergen.id)}
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
      <a href="/admin/ingredients" class="btn btn-ghost">Cancel</a>
      <button type="submit" class="btn btn-primary">Create ingredient</button>
    </div>
  </form>
</div>
