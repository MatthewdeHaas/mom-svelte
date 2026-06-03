<script lang="ts">
  import type { PageData, ActionData } from './$types';
  import { enhance } from '$app/forms';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  let newName = $state('');
  let editingId = $state<string | null>(null);
  let editName = $state('');
  let confirmDeleteId = $state<string | null>(null);
</script>

<style>
  .page-header {
    margin-bottom: 24px;
  }
  .page-header h1 {
    font-size: 22px;
    font-weight: 600;
    color: var(--motm-text);
    letter-spacing: -0.02em;
  }
  .page-header p {
    font-size: 13.5px;
    color: var(--motm-text-muted);
    margin-top: 3px;
  }

  .add-form {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 20px;
  }
  .add-input {
    flex: 1;
    max-width: 300px;
    padding: 8px 12px;
    border: 1px solid var(--motm-border);
    border-radius: 8px;
    font-size: 13.5px;
    font-family: inherit;
    background: #fff;
    color: var(--motm-text);
    outline: none;
    transition: border-color 120ms;
  }
  .add-input:focus { border-color: var(--motm-pink); }
  .add-input::placeholder { color: var(--motm-text-muted); }

  .btn-primary {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    background: var(--motm-pink);
    color: #fff;
    border: none;
    cursor: pointer;
    transition: background 120ms;
  }
  .btn-primary:hover { background: var(--motm-pink-dark); }
  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

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

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: flex-end;
  }
  .btn-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: 1px solid var(--motm-border);
    border-radius: 6px;
    background: #fff;
    cursor: pointer;
    color: var(--motm-text-muted);
    transition: border-color 120ms, color 120ms, background 120ms;
  }
  .btn-icon:hover { border-color: var(--motm-pink); color: var(--motm-pink); }
  .btn-icon.danger:hover { border-color: #E25C5C; color: #E25C5C; }

  .edit-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .edit-input {
    padding: 5px 10px;
    border: 1px solid var(--motm-pink);
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    outline: none;
    color: var(--motm-text);
  }
  .btn-sm {
    padding: 4px 10px;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    border: 1px solid var(--motm-border);
    background: #fff;
    cursor: pointer;
    transition: background 120ms;
  }
  .btn-sm.save {
    background: var(--motm-pink);
    color: #fff;
    border-color: var(--motm-pink);
  }
  .btn-sm.save:hover { background: var(--motm-pink-dark); }
  .btn-sm:hover { background: #FAF8F6; }

  .confirm-delete {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: #E25C5C;
  }
  .btn-danger {
    padding: 4px 10px;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    border: 1px solid #E25C5C;
    background: #E25C5C;
    color: #fff;
    cursor: pointer;
    transition: background 120ms;
  }
  .btn-danger:hover { background: #C94040; }

  .error-msg {
    padding: 10px 14px;
    margin-bottom: 16px;
    border-radius: 8px;
    font-size: 13px;
    background: #FEF2F2;
    color: #B91C1C;
    border: 1px solid #FECACA;
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
  .count {
    font-size: 12.5px;
    color: var(--motm-text-muted);
    margin-bottom: 10px;
  }
</style>

<div class="page-header">
  <h1>Allergens</h1>
  <p>Manage the master allergen list used across ingredients and persons</p>
</div>

{#if form && 'error' in form}
  <div class="error-msg">{form.error}</div>
{/if}

<form class="add-form" method="POST" action="?/create" use:enhance={() => {
  return async ({ update }) => {
    await update();
    newName = '';
  };
}}>
  <input
    class="add-input"
    type="text"
    name="name"
    placeholder="New allergen name..."
    bind:value={newName}
  />
  <button class="btn-primary" type="submit" disabled={!newName.trim()}>
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M12 5v14M5 12h14"/>
    </svg>
    Add
  </button>
</form>

{#if data.allergens.length > 0}
  <p class="count">{data.allergens.length} allergen{data.allergens.length === 1 ? '' : 's'}</p>
{/if}

<div class="table-wrap">
  {#if data.allergens.length === 0}
    <div class="empty-state">
      <p class="empty-title">No allergens yet</p>
      <p>Add your first allergen above.</p>
    </div>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th style="text-align: right;">Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each data.allergens as allergen (allergen.id)}
          <tr>
            <td>
              {#if editingId === allergen.id}
                <form class="edit-row" method="POST" action="?/rename" use:enhance={() => {
                  return async ({ update }) => {
                    await update();
                    editingId = null;
                  };
                }}>
                  <input type="hidden" name="id" value={allergen.id} />
                  <input
                    class="edit-input"
                    type="text"
                    name="name"
                    bind:value={editName}
                  />
                  <button class="btn-sm save" type="submit">Save</button>
                  <button class="btn-sm" type="button" onclick={() => { editingId = null; }}>Cancel</button>
                </form>
              {:else}
                <span style="font-weight: 500;">{allergen.name}</span>
              {/if}
            </td>
            <td>
              {#if confirmDeleteId === allergen.id}
                <div class="confirm-delete">
                  <span>Delete?</span>
                  <form method="POST" action="?/delete" style="display:inline" use:enhance={() => {
                    return async ({ update }) => {
                      await update();
                      confirmDeleteId = null;
                    };
                  }}>
                    <input type="hidden" name="id" value={allergen.id} />
                    <button class="btn-danger" type="submit">Confirm</button>
                  </form>
                  <button class="btn-sm" type="button" onclick={() => { confirmDeleteId = null; }}>Cancel</button>
                </div>
              {:else}
                <div class="actions">
                  <button
                    class="btn-icon"
                    type="button"
                    title="Rename"
                    onclick={() => { editingId = allergen.id; editName = allergen.name; }}
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                      <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                    </svg>
                  </button>
                  <button
                    class="btn-icon danger"
                    type="button"
                    title="Delete"
                    onclick={() => { confirmDeleteId = allergen.id; }}
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="3 6 5 6 21 6"/>
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                    </svg>
                  </button>
                </div>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
