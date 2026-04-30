<script lang="ts">
  import { enhance } from '$app/forms';
  import type { ActionData } from './$types';
  import type { ImportRowError } from '$lib/types';

  export let form: ActionData;

  let file: File | null = null;
  let dragging = false;
  let submitting = false;

  function onFileChange(e: Event) {
    const input = e.target as HTMLInputElement;
    file = input.files?.[0] ?? null;
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    const dropped = e.dataTransfer?.files[0];
    if (dropped?.name.endsWith('.xlsx')) {
      file = dropped;
      // Sync to the hidden input so the form picks it up
      const input = document.getElementById('file-input') as HTMLInputElement;
      const dt = new DataTransfer();
      dt.items.add(dropped);
      input.files = dt.files;
    }
  }

  $: parseErrors = (form as any)?.parseErrors as ImportRowError[] | undefined;
  $: result = (form as any)?.result;
</script>

<svelte:head>
  <title>Import Recipes — Admin</title>
</svelte:head>

<div class="page">
  <header class="page-header">
    <div>
      <h1>Import Recipes</h1>
      <p class="subtitle">
        Upload an Excel spreadsheet to bulk-import recipes and ingredients.
      </p>
    </div>
    <a href="/admin/recipes" class="btn btn-ghost">← Back to Recipes</a>
  </header>

  <!-- Warning banner -->
  <div class="notice notice-warning">
    <svg viewBox="0 0 20 20" fill="currentColor" width="18" height="18" aria-hidden="true">
      <path fill-rule="evenodd" d="M8.485 2.495c.673-1.167 2.357-1.167 3.03 0l6.28 10.875c.673 1.167-.17 2.625-1.516 2.625H3.72c-1.347 0-2.189-1.458-1.515-2.625L8.485 2.495zM10 5a.75.75 0 01.75.75v3.5a.75.75 0 01-1.5 0v-3.5A.75.75 0 0110 5zm0 9a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd"/>
    </svg>
    <span>
      <strong>Re-importing a recipe replaces its ingredient list entirely.</strong>
      Any ingredients not present in the uploaded file will be removed from that recipe.
    </span>
  </div>

  <!-- Template spec -->
  <section class="card">
    <h2 class="card-title">Expected spreadsheet format</h2>
    <p class="card-body-text">
      One row per recipe–ingredient pair. The first row must be a header row with these exact columns:
    </p>
    <div class="table-wrap">
      <table class="spec-table">
        <thead>
          <tr>
            <th>Column</th>
            <th>Required</th>
            <th>Notes</th>
          </tr>
        </thead>
        <tbody>
          <tr><td>Recipe</td><td><span class="badge badge-required">Required</span></td><td>Recipe name — rows with the same name belong to the same recipe</td></tr>
          <tr><td>Ingredient</td><td><span class="badge badge-optional">Optional</span></td><td>Leave blank for a recipe with no ingredients</td></tr>
          <tr><td>Amount</td><td><span class="badge badge-optional">Optional</span></td><td>Positive decimal — required if Ingredient is set</td></tr>
          <tr><td>Unit</td><td><span class="badge badge-optional">Optional</span></td><td>Free text: kg, g, ml, etc. — required if Ingredient is set</td></tr>
          <tr><td>Allergens</td><td><span class="badge badge-optional">Optional</span></td><td>Comma-separated: <code>Dairy, Gluten</code></td></tr>
        </tbody>
      </table>
    </div>
  </section>

  <!-- Upload form -->
  <section class="card">
    <h2 class="card-title">Upload file</h2>

    <form
      method="POST"
      action="?/import"
      enctype="multipart/form-data"
      use:enhance={() => {
        submitting = true;
        return async ({ update }) => {
          await update();
          submitting = false;
        };
      }}
    >
      <!-- Drop zone -->
      <label
        class="dropzone"
        class:dragging
        class:has-file={!!file}
        for="file-input"
        on:dragover|preventDefault={() => (dragging = true)}
        on:dragleave={() => (dragging = false)}
        on:drop={onDrop}
      >
        {#if file}
          <svg viewBox="0 0 20 20" fill="currentColor" width="32" height="32" class="drop-icon drop-icon--file" aria-hidden="true">
            <path fill-rule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4z" clip-rule="evenodd"/>
          </svg>
          <span class="drop-filename">{file.name}</span>
          <span class="drop-meta">{(file.size / 1024).toFixed(1)} KB · click to change</span>
        {:else}
          <svg viewBox="0 0 20 20" fill="currentColor" width="32" height="32" class="drop-icon" aria-hidden="true">
            <path fill-rule="evenodd" d="M3 17a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zM6.293 6.707a1 1 0 010-1.414l3-3a1 1 0 011.414 0l3 3a1 1 0 01-1.414 1.414L11 5.414V13a1 1 0 11-2 0V5.414L7.707 6.707a1 1 0 01-1.414 0z" clip-rule="evenodd"/>
          </svg>
          <span class="drop-label">Drop your <code>.xlsx</code> file here, or click to browse</span>
          <span class="drop-meta">Maximum 5 MB</span>
        {/if}
        <input
          id="file-input"
          type="file"
          name="file"
          accept=".xlsx"
          on:change={onFileChange}
          class="sr-only"
        />
      </label>

      <!-- Global form error -->
      {#if form?.error}
        <div class="alert alert-error" role="alert">
          <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16" aria-hidden="true">
            <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"/>
          </svg>
          {form.error}
        </div>
      {/if}

      <div class="form-footer">
        <button type="submit" class="btn btn-primary" disabled={!file || submitting}>
          {#if submitting}
            <span class="spinner" aria-hidden="true"></span> Importing…
          {:else}
            Import recipes
          {/if}
        </button>
      </div>
    </form>
  </section>

  <!-- Validation errors table -->
  {#if parseErrors?.length}
    <section class="card">
      <h2 class="card-title card-title--error">
        {parseErrors.length} validation {parseErrors.length === 1 ? 'error' : 'errors'} — nothing was imported
      </h2>
      <p class="card-body-text">Fix the rows below in your spreadsheet and re-upload.</p>
      <div class="table-wrap">
        <table class="error-table">
          <thead>
            <tr>
              <th>Row</th>
              <th>Field</th>
              <th>Error</th>
              <th>Raw value</th>
            </tr>
          </thead>
          <tbody>
            {#each parseErrors as err}
              <tr>
                <td class="mono">{err.row}</td>
                <td class="mono">{err.field}</td>
                <td>{err.message}</td>
                <td class="mono truncate" title={err.raw_value}>{err.raw_value}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}

  <!-- Success summary -->
  {#if result}
    <section class="card card--success">
      <h2 class="card-title">Import complete</h2>
      <dl class="result-grid">
        <div class="result-item">
          <dt>Rows processed</dt>
          <dd>{result.rows_processed}</dd>
        </div>
        <div class="result-item">
          <dt>Recipes created</dt>
          <dd>{result.recipes_created}</dd>
        </div>
        <div class="result-item">
          <dt>Recipes updated</dt>
          <dd>{result.recipes_updated}</dd>
        </div>
        <div class="result-item">
          <dt>Ingredients upserted</dt>
          <dd>{result.ingredients_created}</dd>
        </div>
      </dl>
      <div class="card-actions">
        <a href="/admin/recipes" class="btn btn-primary">View recipes</a>
      </div>
    </section>
  {/if}
</div>

<style>
  /* ── Layout ── */
  .page {
    max-width: 860px;
    margin: 0 auto;
    padding: 2rem 1.5rem 4rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  .page-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
  }

  h1 {
    font-size: 1.5rem;
    font-weight: 700;
    margin: 0 0 0.25rem;
    color: var(--color-text, #111);
  }

  .subtitle {
    margin: 0;
    color: var(--color-text-muted, #6b7280);
    font-size: 0.9rem;
  }

  /* ── Notice ── */
  .notice {
    display: flex;
    align-items: flex-start;
    gap: 0.65rem;
    padding: 0.85rem 1rem;
    border-radius: 8px;
    font-size: 0.875rem;
    line-height: 1.5;
  }

  .notice svg { flex-shrink: 0; margin-top: 1px; }

  .notice-warning {
    background: #fffbeb;
    border: 1px solid #fcd34d;
    color: #92400e;
  }

  /* ── Cards ── */
  .card {
    background: var(--color-surface, #fff);
    border: 1px solid var(--color-border, #e5e7eb);
    border-radius: 10px;
    padding: 1.5rem;
  }

  .card--success {
    border-color: #86efac;
    background: #f0fdf4;
  }

  .card-title {
    font-size: 1rem;
    font-weight: 600;
    margin: 0 0 1rem;
    color: var(--color-text, #111);
  }

  .card-title--error { color: #b91c1c; }

  .card-body-text {
    margin: 0 0 1rem;
    font-size: 0.875rem;
    color: var(--color-text-muted, #6b7280);
  }

  .card-actions {
    margin-top: 1.25rem;
  }

  /* ── Tables ── */
  .table-wrap {
    overflow-x: auto;
    border-radius: 6px;
    border: 1px solid var(--color-border, #e5e7eb);
  }

  .spec-table,
  .error-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8125rem;
  }

  .spec-table th,
  .error-table th {
    background: var(--color-bg-subtle, #f9fafb);
    text-align: left;
    padding: 0.5rem 0.75rem;
    font-weight: 600;
    color: var(--color-text-muted, #6b7280);
    border-bottom: 1px solid var(--color-border, #e5e7eb);
    white-space: nowrap;
  }

  .spec-table td,
  .error-table td {
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--color-border, #e5e7eb);
    color: var(--color-text, #111);
    vertical-align: top;
  }

  .spec-table tr:last-child td,
  .error-table tr:last-child td { border-bottom: none; }

  .error-table tr:nth-child(even) td { background: #fef2f2; }

  .mono { font-family: ui-monospace, monospace; font-size: 0.8rem; }

  .truncate {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── Badges ── */
  .badge {
    display: inline-block;
    font-size: 0.7rem;
    font-weight: 600;
    padding: 0.15rem 0.45rem;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .badge-required { background: #fee2e2; color: #b91c1c; }
  .badge-optional { background: #f3f4f6; color: #6b7280; }

  /* ── Dropzone ── */
  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    padding: 2.5rem 1.5rem;
    border: 2px dashed var(--color-border, #d1d5db);
    border-radius: 8px;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    text-align: center;
    margin-bottom: 1rem;
  }

  .dropzone:hover,
  .dropzone.dragging { border-color: var(--color-primary, #2563eb); background: #eff6ff; }
  .dropzone.has-file  { border-color: #86efac; background: #f0fdf4; border-style: solid; }

  .drop-icon { color: var(--color-text-muted, #9ca3af); }
  .drop-icon--file { color: #16a34a; }

  .drop-label {
    font-size: 0.9rem;
    font-weight: 500;
    color: var(--color-text, #374151);
  }

  .drop-filename {
    font-size: 0.9rem;
    font-weight: 600;
    color: #15803d;
    font-family: ui-monospace, monospace;
  }

  .drop-meta { font-size: 0.775rem; color: var(--color-text-muted, #9ca3af); }

  /* ── Alerts ── */
  .alert {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    border-radius: 6px;
    font-size: 0.875rem;
    margin-bottom: 1rem;
  }
  .alert-error { background: #fef2f2; border: 1px solid #fca5a5; color: #b91c1c; }

  /* ── Footer ── */
  .form-footer { display: flex; justify-content: flex-end; }

  /* ── Result grid ── */
  .result-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 1rem;
    margin: 0;
  }

  .result-item {
    background: #fff;
    border: 1px solid #bbf7d0;
    border-radius: 8px;
    padding: 0.75rem 1rem;
  }

  .result-item dt {
    font-size: 0.75rem;
    color: #16a34a;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-bottom: 0.25rem;
  }

  .result-item dd {
    margin: 0;
    font-size: 1.75rem;
    font-weight: 700;
    color: #14532d;
    line-height: 1;
  }

  /* ── Buttons ── */
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem 1.1rem;
    border-radius: 6px;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    border: 1px solid transparent;
    text-decoration: none;
    transition: background 0.12s, opacity 0.12s;
  }

  .btn-primary {
    background: var(--color-primary, #2563eb);
    color: #fff;
    border-color: var(--color-primary, #2563eb);
  }
  .btn-primary:hover:not(:disabled) { filter: brightness(1.08); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-ghost {
    background: transparent;
    color: var(--color-text-muted, #6b7280);
    border-color: var(--color-border, #d1d5db);
  }
  .btn-ghost:hover { background: var(--color-bg-subtle, #f9fafb); }

  /* ── Spinner ── */
  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255,255,255,0.4);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* ── Accessibility ── */
  .sr-only {
    position: absolute;
    width: 1px; height: 1px;
    padding: 0; margin: -1px;
    overflow: hidden; clip: rect(0,0,0,0);
    white-space: nowrap; border: 0;
  }

  code {
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
    background: rgba(0,0,0,0.06);
    padding: 0.1em 0.3em;
    border-radius: 3px;
  }
</style>
