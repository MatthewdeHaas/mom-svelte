<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData, ActionData } from './$types';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  const activePersons = $derived(data.persons.filter((p) => p.active));
  const inactivePersons = $derived(data.persons.filter((p) => !p.active));
</script>

<style>
  .header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 28px;
    flex-wrap: wrap;
  }
  h1 { font-size: 22px; font-weight: 600; color: #2C1F24; letter-spacing: -0.02em; margin: 0 0 4px; }
  .sub { font-size: 14px; color: #7A6670; margin: 0; }

  /* Add person card */
  .add-card {
    background: #fff;
    border: 1px solid #EDE8E3;
    border-radius: 10px;
    padding: 20px 22px;
    margin-bottom: 24px;
    display: flex;
    align-items: flex-end;
    gap: 12px;
    flex-wrap: wrap;
  }
  .add-field { display: flex; flex-direction: column; gap: 5px; flex: 1; min-width: 200px; }
  label { font-size: 12.5px; font-weight: 600; color: #2C1F24; }
  input {
    padding: 8px 11px;
    border: 1px solid #EDE8E3;
    border-radius: 7px;
    font-size: 13.5px;
    font-family: inherit;
    color: #2C1F24;
    background: #fff;
    outline: none;
    transition: border-color 120ms;
  }
  input:focus { border-color: #C9718A; }
  input.error { border-color: #E24B4A; }
  .field-error { font-size: 12px; color: #A32D2D; margin-top: 2px; }

  .btn {
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms;
    white-space: nowrap;
  }
  .btn-primary { background: #C9718A; color: #fff; }
  .btn-primary:hover { background: #b05e77; }

  /* Person list */
  .section-label {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: #B8AEB3;
    margin: 0 0 10px;
  }
  .person-list { display: flex; flex-direction: column; gap: 8px; margin-bottom: 28px; }
  .person-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: #fff;
    border: 1px solid #EDE8E3;
    border-radius: 10px;
    padding: 14px 18px;
    text-decoration: none;
    gap: 12px;
    transition: border-color 120ms, background 120ms;
  }
  .person-row:hover { border-color: #C9718A; background: #FFF8FA; }
  .person-inactive { opacity: 0.55; }
  .person-name { font-size: 14px; font-weight: 500; color: #2C1F24; }
  .allergen-list { display: flex; flex-wrap: wrap; gap: 4px; }
  .allergen-tag {
    padding: 2px 8px;
    background: #FEF3C7;
    border: 1px solid #FDE68A;
    border-radius: 20px;
    font-size: 11.5px;
    font-weight: 500;
    color: #92400E;
  }
  .none-tag { font-size: 12.5px; color: #B8AEB3; }
  .chevron { color: #B8AEB3; flex-shrink: 0; }

  .empty { font-size: 14px; color: #B8AEB3; padding: 24px 0; }
</style>

<div class="header">
  <div>
    <h1>Roster</h1>
    <p class="sub">Manage the people on your meal programme and their allergen profiles.</p>
  </div>
</div>

<!-- Add person -->
<form method="POST" action="?/create" use:enhance class="add-card">
  <div class="add-field">
    <label for="name">Add a person</label>
    <input
      id="name"
      name="name"
      type="text"
      placeholder="Full name"
      value={form?.action === 'create' ? ((form?.values as Record<string, string>)?.name ?? '') : ''}
      class:error={form?.action === 'create' && form?.errors?.name}
      required
    />
    {#if form?.action === 'create' && form?.errors?.name}
      <span class="field-error">{(form.errors as Record<string, string>).name}</span>
    {/if}
  </div>
  <button type="submit" class="btn btn-primary">Add person</button>
</form>

<!-- Active persons -->
{#if activePersons.length > 0}
  <p class="section-label">{activePersons.length} active</p>
  <div class="person-list">
    {#each activePersons as person}
      <a href="/client/roster/{person.id}" class="person-row">
        <div>
          <p class="person-name">{person.name}</p>
          {#if person.allergen.length > 0}
            <div class="allergen-list" style="margin-top: 4px;">
              {#each person.allergen as a}
                <span class="allergen-tag">{a.name}</span>
              {/each}
            </div>
          {:else}
            <span class="none-tag">No allergens recorded</span>
          {/if}
        </div>
        <svg class="chevron" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M9 18l6-6-6-6"/>
        </svg>
      </a>
    {/each}
  </div>
{:else}
  <p class="empty">No people added yet. Use the form above to add your first person.</p>
{/if}

<!-- Inactive persons (collapsible) -->
{#if inactivePersons.length > 0}
  <details>
    <summary class="section-label" style="cursor:pointer;list-style:none;user-select:none;">
      {inactivePersons.length} inactive ▸
    </summary>
    <div class="person-list" style="margin-top:10px;">
      {#each inactivePersons as person}
        <a href="/client/roster/{person.id}" class="person-row person-inactive">
          <p class="person-name">{person.name}</p>
          <svg class="chevron" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M9 18l6-6-6-6"/>
          </svg>
        </a>
      {/each}
    </div>
  </details>
{/if}
