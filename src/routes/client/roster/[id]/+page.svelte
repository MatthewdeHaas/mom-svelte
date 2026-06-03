<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData, ActionData } from './$types';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  const person = $derived(data.person);

  // svelte-ignore state_referenced_locally — intentional one-time capture; $effect below
  // handles re-sync on navigation to a different person
  let nameValue = $state(data.person.name);
  // svelte-ignore state_referenced_locally
  let trackedPersonId = data.person.id;

  $effect(() => {
    if (data.person.id !== trackedPersonId) {
      nameValue = data.person.name;
      trackedPersonId = data.person.id;
    }
  });

  const selectedAllergenIds = $derived(
    new Set(
      form?.action === 'update'
        ? ((form?.values as Record<string, string | string[]>)?.allergen_ids as string[] ?? [])
        : person.allergen.map((a) => a.id)
    )
  );
</script>

<style>
  .back-link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 13px;
    color: #7A6670;
    text-decoration: none;
    margin-bottom: 20px;
    transition: color 120ms;
  }
  .back-link:hover { color: #2C1F24; }

  .page-header { margin-bottom: 24px; }
  h1 { font-size: 22px; font-weight: 600; color: #2C1F24; letter-spacing: -0.02em; margin: 0 0 4px; display: flex; align-items: center; gap: 10px; }
  .inactive-chip {
    font-size: 12px; font-weight: 600; padding: 3px 9px; border-radius: 5px;
    background: #F3F3F1; color: #888;
  }

  .layout { display: grid; grid-template-columns: 1fr 300px; gap: 20px; align-items: start; }
  @media (max-width: 720px) { .layout { grid-template-columns: 1fr; } }

  .card { background: #fff; border: 1px solid #EDE8E3; border-radius: 10px; margin-bottom: 16px; }
  .card-header { padding: 18px 22px 14px; border-bottom: 1px solid #EDE8E3; }
  .card-title { font-size: 14px; font-weight: 600; color: #2C1F24; }
  .card-sub { font-size: 12.5px; color: #7A6670; margin-top: 2px; }
  .card-body { padding: 22px; display: flex; flex-direction: column; gap: 16px; }
  .card-footer {
    padding: 14px 22px; border-top: 1px solid #EDE8E3;
    display: flex; gap: 10px; justify-content: flex-end; align-items: center;
  }

  .field { display: flex; flex-direction: column; gap: 5px; }
  label, .field-label { font-size: 12.5px; font-weight: 600; color: #2C1F24; }
  input {
    padding: 8px 11px; border: 1px solid #EDE8E3; border-radius: 7px;
    font-size: 13.5px; font-family: inherit; color: #2C1F24; background: #fff;
    outline: none; transition: border-color 120ms;
  }
  input:focus { border-color: #C9718A; }
  input.error { border-color: #E24B4A; }
  .field-error { font-size: 12px; color: #A32D2D; }

  .allergen-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 8px; }
  .allergen-check {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border: 1px solid #EDE8E3;
    border-radius: 8px;
    cursor: pointer;
    font-size: 13px;
    color: #2C1F24;
    transition: border-color 120ms, background 120ms;
    user-select: none;
  }
  .allergen-check:has(input:checked) {
    border-color: #C9718A;
    background: #FFF8FA;
  }
  .allergen-check input[type="checkbox"] {
    width: 15px; height: 15px; border-radius: 4px;
    accent-color: #C9718A;
    padding: 0; border: none; flex-shrink: 0;
  }

  .btn {
    padding: 8px 16px; border-radius: 8px; font-size: 13.5px; font-weight: 500;
    font-family: inherit; cursor: pointer; border: none; transition: background 120ms;
    display: inline-flex; align-items: center; gap: 6px;
  }
  .btn-primary { background: #C9718A; color: #fff; }
  .btn-primary:hover { background: #b05e77; }
  .btn-danger { background: transparent; color: #A32D2D; border: 1px solid #F09595; }
  .btn-danger:hover { background: #FCEBEB; }
  .btn-ghost { background: transparent; color: #7A6670; border: 1px solid #EDE8E3; }
  .btn-ghost:hover { background: #FAF8F6; color: #2C1F24; }

  .success-strip {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 14px; background: #D1FAE5; color: #1F6648;
    border-radius: 7px; font-size: 13px; font-weight: 500; margin-bottom: 16px;
  }

  .danger-zone { border-color: #F0CBCB; }
  .danger-zone .card-header { border-bottom-color: #F0CBCB; }
  .danger-body { padding: 18px 22px; display: flex; align-items: center; justify-content: space-between; gap: 16px; flex-wrap: wrap; }
  .danger-desc { font-size: 13px; color: #7A6670; max-width: 300px; line-height: 1.55; }
</style>

<a href="/client/roster" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Roster
</a>

<div class="page-header">
  <h1>
    {nameValue}
    {#if !person.active}<span class="inactive-chip">Inactive</span>{/if}
  </h1>
</div>

{#if form?.action === 'update' && form?.success}
  <div class="success-strip">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M20 6 9 17l-5-5"/>
    </svg>
    Changes saved
  </div>
{/if}
{#if form?.action === 'reactivate' && form?.success}
  <div class="success-strip">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M20 6 9 17l-5-5"/>
    </svg>
    Person reactivated
  </div>
{/if}

<div class="layout">
  <div>
    <!-- Edit form -->
    <div class="card">
      <div class="card-header">
        <p class="card-title">Details &amp; allergens</p>
        <p class="card-sub">Check all allergens that apply to this person.</p>
      </div>
      <form method="POST" action="?/update" use:enhance={() => {
        return async ({ update }) => { await update({ reset: false }); };
      }}>
        <div class="card-body">
          <div class="field">
            <label for="name">Name</label>
            <input
              id="name"
              name="name"
              type="text"
              bind:value={nameValue}
              class:error={form?.action === 'update' && (form?.errors as Record<string, string>)?.name}
              disabled={!person.active}
              required
            />
            {#if form?.action === 'update' && (form?.errors as Record<string, string>)?.name}
              <span class="field-error">{(form.errors as Record<string, string>).name}</span>
            {/if}
          </div>

          <div class="field">
            <span class="field-label">Allergens</span>
            <div class="allergen-grid">
              {#each data.allAllergens as allergen}
                <label class="allergen-check">
                  <input
                    type="checkbox"
                    name="allergen_ids"
                    value={allergen.id}
                    checked={selectedAllergenIds.has(allergen.id)}
                    disabled={!person.active}
                  />
                  {allergen.name}
                </label>
              {/each}
            </div>
          </div>
        </div>
        <div class="card-footer">
          {#if person.active}
            <button type="submit" class="btn btn-primary">Save changes</button>
          {:else}
            <span style="font-size:12.5px;color:#7A6670;">Reactivate to edit</span>
          {/if}
        </div>
      </form>
    </div>

    <!-- Danger / reactivate zone -->
    {#if person.active}
      <div class="card danger-zone">
        <div class="card-header">
          <p class="card-title" style="color:#A32D2D;">Remove from roster</p>
        </div>
        <div class="danger-body">
          <p class="danger-desc">
            Deactivating removes this person from the active roster. Their attendance history is preserved.
          </p>
          <form method="POST" action="?/deactivate" use:enhance>
            <button type="submit" class="btn btn-danger">Deactivate</button>
          </form>
        </div>
      </div>
    {:else}
      <div class="card">
        <div class="danger-body">
          <p class="danger-desc">This person is inactive and won't appear in attendance or allergen views.</p>
          <form method="POST" action="?/reactivate" use:enhance>
            <button type="submit" class="btn btn-ghost">Reactivate</button>
          </form>
        </div>
      </div>
    {/if}
  </div>

  <!-- Right: info panel -->
  <div class="card">
    <div class="card-header">
      <p class="card-title">Current allergens</p>
    </div>
    <div class="card-body" style="padding: 16px 18px;">
      {#if person.allergen.length === 0}
        <p style="font-size:13px;color:#B8AEB3;">None recorded</p>
      {:else}
        <div style="display:flex;flex-wrap:wrap;gap:6px;">
          {#each person.allergen as a}
            <span style="padding:3px 10px;background:#FEF3C7;border:1px solid #FDE68A;border-radius:20px;font-size:12px;font-weight:500;color:#92400E;">
              {a.name}
            </span>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
