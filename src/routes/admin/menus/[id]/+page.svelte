<script lang="ts">
  import type { PageData, ActionData } from './$types';
  import { enhance } from '$app/forms';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  const DAYS = [
    { label: 'Mon', value: 0 },
    { label: 'Tue', value: 1 },
    { label: 'Wed', value: 2 },
    { label: 'Thu', value: 3 },
    { label: 'Fri', value: 4 },
    { label: 'Sat', value: 5 },
    { label: 'Sun', value: 6 },
  ];

  const MEAL_CATEGORIES = ['Breakfast', 'Lunch', 'Snack', 'Dinner'];

  let activeWeek = $state(1);

  // Slide-over state
  type PanelTarget = {
    cycle_week: number;
    day_of_week: number;
    meal_category: string;
    current_recipe_id: string | null;
    current_recipe_name: string | null;
    day_label: string;
  };

  let panelOpen = $state(false);
  let panelTarget = $state<PanelTarget | null>(null);
  let recipeSearch = $state('');

  const filteredRecipes = $derived(
    recipeSearch.trim().length === 0
      ? data.recipes
      : data.recipes.filter((r: any) =>
          r.name.toLowerCase().includes(recipeSearch.trim().toLowerCase())
        )
  );

  function openPanel(
    week: number,
    day: number,
    meal: string,
    dayLabel: string,
    currentId: string | null,
    currentName: string | null
  ) {
    panelTarget = {
      cycle_week: week,
      day_of_week: day,
      meal_category: meal,
      current_recipe_id: currentId,
      current_recipe_name: currentName,
      day_label: dayLabel,
    };
    recipeSearch = '';
    panelOpen = true;
  }

  function closePanel() {
    panelOpen = false;
    panelTarget = null;
    recipeSearch = '';
  }

  function getSlot(week: number, day: number, meal: string) {
    return data.slots.find(
      (s: any) =>
        s.cycle_week === week &&
        s.day_of_week === day &&
        s.meal_category.toLowerCase() === meal.toLowerCase()
    ) ?? null;
  }

  function formatDate(val: string | Date | null): string {
    if (!val) return '—';
    return new Date(val).toLocaleDateString('en-GB', {
      day: 'numeric', month: 'short', year: 'numeric',
    });
  }

  // Close panel after successful action
  $effect(() => {
    if (form?.success && (form?.action === 'assignSlot' || form?.action === 'clearSlot')) {
      closePanel();
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

  .page-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    margin-bottom: 24px;
    gap: 16px;
    flex-wrap: wrap;
  }
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
    font-size: 12px; font-weight: 600; padding: 3px 9px;
    border-radius: 5px; background: #F3F3F1; color: #888;
  }
  .active-chip {
    font-size: 12px; font-weight: 600; padding: 3px 9px;
    border-radius: 5px; background: #D1FAE5; color: #1F6648;
  }

  /* Cards */
  .card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    margin-bottom: 20px;
  }
  .card-header { padding: 18px 22px 14px; border-bottom: 1px solid var(--motm-border); }
  .card-title { font-size: 14px; font-weight: 600; color: var(--motm-text); }
  .card-body { padding: 22px; display: flex; flex-direction: column; gap: 16px; }
  .card-footer {
    padding: 14px 22px;
    border-top: 1px solid var(--motm-border);
    display: flex; gap: 10px;
    justify-content: flex-end;
    align-items: center;
  }

  /* Details form */
  .field-row { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .field { display: flex; flex-direction: column; gap: 5px; }
  label { font-size: 12.5px; font-weight: 600; color: var(--motm-text); }
  .label-optional { font-weight: 400; color: var(--motm-text-muted); margin-left: 4px; }
  .field-error { font-size: 12px; color: #A32D2D; }
  input[type="text"], input[type="date"] {
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
  input:focus { border-color: var(--motm-pink); }
  input.error { border-color: #E24B4A; }
  input:disabled { background: #FAF8F6; color: var(--motm-text-muted); cursor: not-allowed; }

  /* Week tabs */
  .week-tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--motm-border);
    padding: 0 22px;
  }
  .week-tab {
    padding: 11px 18px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    color: var(--motm-text-muted);
    background: transparent;
    border: none;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color 120ms, border-color 120ms;
    white-space: nowrap;
  }
  .week-tab:hover { color: var(--motm-text); }
  .week-tab.active {
    color: var(--motm-pink-dark);
    border-bottom-color: var(--motm-pink);
    font-weight: 600;
  }

  /* Grid */
  .grid-scroll { overflow-x: auto; }
  .grid-table {
    width: 100%;
    border-collapse: collapse;
    min-width: 600px;
  }
  .grid-table th {
    padding: 10px 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--motm-text-muted);
    text-align: center;
    border-bottom: 1px solid var(--motm-border);
    background: #FAF8F6;
  }
  .grid-table th.meal-header {
    text-align: left;
    padding-left: 18px;
    width: 100px;
  }
  .grid-table td {
    border-right: 1px solid var(--motm-border);
    border-bottom: 1px solid var(--motm-border);
    padding: 0;
    vertical-align: stretch;
  }
  .grid-table td:last-child { border-right: none; }
  .grid-table td.meal-label {
    padding: 0 18px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--motm-text-muted);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    background: #FAF8F6;
    border-left: 1px solid var(--motm-border);
    white-space: nowrap;
  }

  /* Cell */
  .cell-filled {
    min-height: 68px;
    padding: 9px 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    cursor: pointer;
    transition: background 120ms;
    width: 100%;
    height: 100%;
    text-align: left;
    border: none;
    background: transparent;
    font-family: inherit;
  }
  .cell-filled:hover { background: #FDF5F7; }
  .cell-recipe-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--motm-text);
    line-height: 1.3;
    flex: 1;
  }
  .cell-edit-hint {
    font-size: 11px;
    color: var(--motm-pink);
    opacity: 0;
    transition: opacity 120ms;
    font-weight: 500;
  }
  .cell-filled:hover .cell-edit-hint { opacity: 1; }

  .cell-empty {
    min-height: 68px;
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: #D0C8CB;
    font-size: 12px;
    font-weight: 500;
    gap: 4px;
    transition: background 120ms, color 120ms;
    border: none;
    background: transparent;
    font-family: inherit;
  }
  .cell-empty:hover { background: #FDF5F7; color: var(--motm-pink); }
  .cell-empty:disabled { cursor: default; }
  .cell-empty:disabled:hover { background: transparent; color: #D0C8CB; }

  /* Slide-over overlay */
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(46, 21, 33, 0.35);
    z-index: 40;
    backdrop-filter: blur(1px);
  }

  /* Slide-over panel */
  .panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: 380px;
    max-width: 95vw;
    background: #fff;
    z-index: 50;
    display: flex;
    flex-direction: column;
    box-shadow: -4px 0 24px rgba(46,21,33,0.12);
  }

  .panel-header {
    padding: 20px 22px 16px;
    border-bottom: 1px solid var(--motm-border);
    flex-shrink: 0;
  }
  .panel-context {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--motm-pink-dark);
    margin-bottom: 4px;
  }
  .panel-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--motm-text);
    letter-spacing: -0.01em;
  }
  .panel-current {
    margin-top: 6px;
    font-size: 12.5px;
    color: var(--motm-text-muted);
  }
  .panel-current strong { color: var(--motm-text); font-weight: 500; }

  .panel-close {
    position: absolute;
    top: 18px;
    right: 18px;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--motm-text-muted);
    padding: 4px;
    border-radius: 6px;
    transition: background 120ms, color 120ms;
    display: flex;
    align-items: center;
  }
  .panel-close:hover { background: #F3EEF0; color: var(--motm-text); }

  /* Search */
  .panel-search {
    padding: 14px 16px;
    border-bottom: 1px solid var(--motm-border);
    flex-shrink: 0;
  }
  .search-wrap {
    position: relative;
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
    padding: 8px 12px 8px 32px;
    border: 1px solid var(--motm-border);
    border-radius: 8px;
    font-size: 13.5px;
    font-family: inherit;
    background: #FAF8F6;
    color: var(--motm-text);
    outline: none;
    transition: border-color 120ms, background 120ms;
    box-sizing: border-box;
  }
  .search-input:focus { border-color: var(--motm-pink); background: #fff; }
  .search-input::placeholder { color: var(--motm-text-muted); }

  .search-count {
    font-size: 11.5px;
    color: var(--motm-text-muted);
    margin-top: 6px;
  }

  /* Recipe list */
  .panel-list {
    flex: 1;
    overflow-y: auto;
    padding: 6px 0;
  }
  .recipe-option {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 10px 16px;
    font-size: 13.5px;
    font-family: inherit;
    color: var(--motm-text);
    background: transparent;
    border: none;
    text-align: left;
    cursor: pointer;
    transition: background 100ms;
    gap: 10px;
  }
  .recipe-option:hover { background: #FAF8F6; }
  .recipe-option.selected { background: var(--motm-pink-light); color: var(--motm-pink-dark); font-weight: 500; }
  .recipe-option.selected:hover { background: #EFD0DB; }
  .selected-tick {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    color: var(--motm-pink-dark);
  }
  .unselected-spacer { width: 16px; flex-shrink: 0; }

  .no-results {
    padding: 32px 16px;
    text-align: center;
    color: var(--motm-text-muted);
    font-size: 13.5px;
  }

  /* Panel footer */
  .panel-footer {
    padding: 14px 16px;
    border-top: 1px solid var(--motm-border);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .panel-assign-form { display: contents; }

  /* Buttons */
  .btn {
    padding: 9px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms, color 120ms;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    text-decoration: none;
    width: 100%;
  }
  .btn-primary { background: var(--motm-pink); color: #fff; }
  .btn-primary:hover { background: var(--motm-pink-dark); }
  .btn-sm { padding: 7px 14px; font-size: 13px; }
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

  /* Strips */
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

<a href="/admin/menus" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Menus
</a>

<div class="page-header">
  <div>
    <h1 class="page-title">
      {data.menu.name}
      {#if data.menu.is_active}
        <span class="active-chip">Active</span>
      {:else}
        <span class="inactive-chip">Inactive</span>
      {/if}
    </h1>
    <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">
      {formatDate(data.menu.start_date)} — {formatDate(data.menu.end_date)}
    </p>
  </div>
</div>

<!-- Status strips -->
{#if form?.action === 'updateDetails' && form?.success}
  <div class="strip strip-success">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Details saved
  </div>
{/if}
{#if form?.action === 'reactivate' && form?.success}
  <div class="strip strip-success">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Menu reactivated
  </div>
{/if}
{#if form?.action === 'deactivate' && form?.success}
  <div class="strip strip-neutral">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Menu deactivated
  </div>
{/if}

<!-- Details card -->
<div class="card">
  <div class="card-header">
    <p class="card-title">Menu details</p>
  </div>
  <form method="POST" action="?/updateDetails" use:enhance>
    <div class="card-body">
      <div class="field">
        <label for="name">Name</label>
        <input
          id="name"
          name="name"
          type="text"
          class:error={form?.action === 'updateDetails' && form?.errors?.name}
          value={form?.action === 'updateDetails'
            ? (form?.values?.name ?? data.menu.name)
            : data.menu.name}
          disabled={!data.menu.is_active}
          required
        />
        {#if form?.action === 'updateDetails' && form?.errors?.name}
          <span class="field-error">{form.errors.name}</span>
        {/if}
      </div>
      <div class="field-row">
        <div class="field">
          <label for="start_date">
            Start date <span class="label-optional">(optional)</span>
          </label>
          <input
            id="start_date"
            name="start_date"
            type="date"
            value={form?.action === 'updateDetails'
              ? (form?.values?.start_date ?? (data.menu.start_date ? new Date(data.menu.start_date).toISOString().slice(0,10) : ''))
              : (data.menu.start_date ? new Date(data.menu.start_date).toISOString().slice(0,10) : '')}
            disabled={!data.menu.is_active}
          />
        </div>
        <div class="field">
          <label for="end_date">
            End date <span class="label-optional">(optional)</span>
          </label>
          <input
            id="end_date"
            name="end_date"
            type="date"
            class:error={form?.action === 'updateDetails' && form?.errors?.end_date}
            value={form?.action === 'updateDetails'
              ? (form?.values?.end_date ?? (data.menu.end_date ? new Date(data.menu.end_date).toISOString().slice(0,10) : ''))
              : (data.menu.end_date ? new Date(data.menu.end_date).toISOString().slice(0,10) : '')}
            disabled={!data.menu.is_active}
          />
          {#if form?.action === 'updateDetails' && form?.errors?.end_date}
            <span class="field-error">{form.errors.end_date}</span>
          {/if}
        </div>
      </div>
    </div>
    <div class="card-footer">
      {#if !data.menu.is_active}
        <span style="font-size: 12.5px; color: var(--motm-text-muted);">Inactive menus cannot be edited</span>
        <form method="POST" action="?/reactivate" use:enhance>
          <button type="submit" class="btn btn-ghost btn-sm" style="width: auto;">Reactivate menu</button>
        </form>
      {:else}
        <button type="submit" class="btn btn-primary btn-sm" style="width: auto;">Save details</button>
      {/if}
    </div>
  </form>
</div>

<!-- Cycle grid card -->
<div class="card">
  <div class="week-tabs">
    {#each [1, 2, 3, 4] as week}
      <button
        class="week-tab"
        class:active={activeWeek === week}
        onclick={() => (activeWeek = week)}
        type="button"
      >
        Week {week}
      </button>
    {/each}
  </div>

  <div class="grid-scroll">
    <table class="grid-table">
      <thead>
        <tr>
          <th class="meal-header"></th>
          {#each DAYS as day}
            <th>{day.label}</th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each MEAL_CATEGORIES as meal}
          <tr>
            <td class="meal-label">{meal}</td>
            {#each DAYS as day}
              {@const slot = getSlot(activeWeek, day.value, meal)}
              <td>
                {#if slot}
                  <button
                    type="button"
                    class="cell-filled"
                    onclick={() => openPanel(activeWeek, day.value, meal, day.label, slot.recipe_id, slot.recipe_name)}
                    disabled={!data.menu.is_active}
                  >
                    <span class="cell-recipe-name">{slot.recipe_name}</span>
                    {#if data.menu.is_active}
                      <span class="cell-edit-hint">Click to change</span>
                    {/if}
                  </button>
                {:else}
                  <button
                    type="button"
                    class="cell-empty"
                    onclick={() => openPanel(activeWeek, day.value, meal, day.label, null, null)}
                    disabled={!data.menu.is_active}
                  >
                    {#if data.menu.is_active}
                      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                        <path d="M12 5v14M5 12h14"/>
                      </svg>
                      Assign
                    {:else}
                      —
                    {/if}
                  </button>
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<!-- Danger zone -->
{#if data.menu.is_active}
  <div class="card danger-zone">
    <div class="card-header">
      <p class="card-title" style="color: #A32D2D;">Danger zone</p>
    </div>
    <div class="danger-zone-body">
      <p class="danger-desc">
        Deactivating this menu removes it from scheduling. Existing orders are unaffected.
      </p>
      <form method="POST" action="?/deactivate" use:enhance>
        <button type="submit" class="btn btn-danger" style="width: auto;">Deactivate menu</button>
      </form>
    </div>
  </div>
{/if}

<!-- Slide-over overlay -->
{#if panelOpen && panelTarget}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="overlay" onclick={closePanel}></div>

  <div class="panel">
    <div class="panel-header">
      <p class="panel-context">
        Week {panelTarget.cycle_week} · {panelTarget.day_label} · {panelTarget.meal_category}
      </p>
      <p class="panel-title">Assign recipe</p>
      {#if panelTarget.current_recipe_name}
        <p class="panel-current">
          Currently: <strong>{panelTarget.current_recipe_name}</strong>
        </p>
      {:else}
        <p class="panel-current">No recipe assigned</p>
      {/if}
      <button class="panel-close" onclick={closePanel} type="button" aria-label="Close">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M18 6 6 18M6 6l12 12"/>
        </svg>
      </button>
    </div>

    <div class="panel-search">
      <div class="search-wrap">
        <svg class="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
        </svg>
        <input
          class="search-input"
          type="search"
          placeholder="Search recipes…"
          bind:value={recipeSearch}
          autofocus
        />
      </div>
      <p class="search-count">
        {filteredRecipes.length} of {data.recipes.length} recipes
      </p>
    </div>

    <div class="panel-list">
      {#if filteredRecipes.length === 0}
        <div class="no-results">No recipes match "{recipeSearch}"</div>
      {:else}
        {#each filteredRecipes as recipe}
          {@const isCurrent = recipe.id === panelTarget.current_recipe_id}
          <form
            method="POST"
            action="?/assignSlot"
            use:enhance
            class="panel-assign-form"
          >
            <input type="hidden" name="cycle_week" value={panelTarget.cycle_week} />
            <input type="hidden" name="day_of_week" value={panelTarget.day_of_week} />
            <input type="hidden" name="meal_category" value={panelTarget.meal_category} />
            <input type="hidden" name="recipe_id" value={recipe.id} />
            <button
              type="submit"
              class="recipe-option"
              class:selected={isCurrent}
            >
              {#if isCurrent}
                <svg class="selected-tick" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                  <path d="M20 6 9 17l-5-5"/>
                </svg>
              {:else}
                <span class="unselected-spacer"></span>
              {/if}
              {recipe.name}
            </button>
          </form>
        {/each}
      {/if}
    </div>

    <!-- Clear slot footer — only shown if a recipe is assigned -->
    {#if panelTarget.current_recipe_id}
      <div class="panel-footer">
        <form method="POST" action="?/clearSlot" use:enhance>
          <input type="hidden" name="cycle_week" value={panelTarget.cycle_week} />
          <input type="hidden" name="day_of_week" value={panelTarget.day_of_week} />
          <input type="hidden" name="meal_category" value={panelTarget.meal_category} />
          <button type="submit" class="btn btn-danger">Clear slot</button>
        </form>
      </div>
    {/if}
  </div>
{/if}
