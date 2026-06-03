<script lang="ts">
  import type { ActionData } from './$types';
  import type { ImportRowError } from '$lib/types';
  import { enhance } from '$app/forms';

  const { form }: { form: ActionData } = $props();

  let importFile = $state<File | null>(null);
  let dragging = $state(false);
  let submitting = $state(false);
  let mode = $state<'replace' | 'add'>('replace');
  let confirmed = $state(false);

  function onFileChange(e: Event) {
    const input = e.target as HTMLInputElement;
    importFile = input.files?.[0] ?? null;
    confirmed = false;
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    const dropped = e.dataTransfer?.files[0];
    if (dropped?.name.endsWith('.xlsx')) {
      importFile = dropped;
      const input = document.getElementById('import-file-input') as HTMLInputElement;
      const dt = new DataTransfer();
      dt.items.add(dropped);
      input.files = dt.files;
      confirmed = false;
    }
  }

  const parseErrors = $derived(
    (form as any)?.parseErrors as Array<{ row: number; field: string; message: string; raw_value: string }> | undefined
  );
  const warnings = $derived((form as any)?.warnings as ImportRowError[] | undefined);
  const importResult = $derived((form as any)?.result);
  const importError = $derived((form as any)?.importError as string | undefined);

  function handleProceedAnyway() {
    confirmed = true;
    const formEl = document.getElementById('import-form') as HTMLFormElement;
    formEl?.requestSubmit();
  }
</script>

<style>
  /* ── Shared styles ── */
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
    max-width: 480px;
  }
  .card-header {
    padding: 20px 24px 16px;
    border-bottom: 1px solid var(--motm-border);
  }
  .card-title { font-size: 15px; font-weight: 600; color: var(--motm-text); }
  .card-sub { font-size: 12.5px; color: var(--motm-text-muted); margin-top: 3px; }
  .card-body { padding: 24px; }
  .card-footer {
    padding: 16px 24px;
    border-top: 1px solid var(--motm-border);
    display: flex;
    gap: 10px;
    justify-content: flex-end;
  }

  .field { display: flex; flex-direction: column; gap: 5px; }
  label { font-size: 12.5px; font-weight: 600; color: var(--motm-text); }
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
    gap: 6px;
  }
  .btn-primary { background: var(--motm-pink); color: #fff; }
  .btn-primary:hover:not(:disabled) { background: var(--motm-pink-dark); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-ghost {
    background: transparent;
    color: var(--motm-text-muted);
    border: 1px solid var(--motm-border);
  }
  .btn-ghost:hover { background: #FAF8F6; color: var(--motm-text); }

  /* ── Page layout ── */
  .page-header { margin-bottom: 24px; }
  .page-title { font-size: 22px; font-weight: 600; color: var(--motm-text); letter-spacing: -0.02em; }
  .page-sub { font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px; }

  .sections {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 480px;
  }

  @media (min-width: 900px) {
    .sections {
      flex-direction: row;
      align-items: flex-start;
      max-width: none;
    }

    .sections .card {
      flex: 1;
      max-width: none;
    }
  }

  /* ── Divider ── */
  .divider {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--motm-text-muted);
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .divider::before,
  .divider::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--motm-border);
  }

  @media (min-width: 900px) {
    .divider {
      flex-direction: column;
      align-self: stretch;
    }
    .divider::before,
    .divider::after {
      width: 1px;
      height: auto;
      flex: 1;
    }
  }

  /* ── Notice ── */
  .notice {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    background: #FFF9EC;
    border: 1px solid #F0D070;
    border-radius: 7px;
    padding: 10px 12px;
    font-size: 12px;
    color: #7A5500;
    line-height: 1.5;
  }
  .notice svg { flex-shrink: 0; margin-top: 1px; }

  /* ── Mode toggle ── */
  .mode-toggle {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .mode-option {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 8px 12px;
    border: 1px solid var(--motm-border);
    border-radius: 7px;
    cursor: pointer;
    transition: border-color 120ms, background 120ms;
    flex: 1;
    min-width: 120px;
  }
  .mode-option input[type="radio"] { display: none; }
  .mode-option strong { font-size: 12.5px; color: var(--motm-text); }
  .mode-option span { font-size: 11px; color: var(--motm-text-muted); }
  .mode-option:hover { border-color: var(--motm-pink); }
  .mode-option.active {
    border-color: var(--motm-pink);
    background: #FDF5F7;
  }

  /* ── Dropzone ── */
  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 20px 16px;
    border: 1.5px dashed var(--motm-border);
    border-radius: 8px;
    cursor: pointer;
    transition: border-color 120ms, background 120ms;
    text-align: center;
  }
  .dropzone:hover,
  .dropzone.dragging {
    border-color: var(--motm-pink);
    background: #FDF5F7;
  }
  .dropzone.has-file {
    border-style: solid;
    border-color: #86efac;
    background: #F0FDF4;
  }

  .drop-icon { color: var(--motm-text-muted); }
  .drop-icon--file { color: #16a34a; }
  .drop-label { font-size: 13px; color: var(--motm-text); font-weight: 500; }
  .drop-filename { font-size: 12.5px; font-weight: 600; color: #15803d; font-family: ui-monospace, monospace; }
  .drop-meta { font-size: 11.5px; color: var(--motm-text-muted); }

  /* ── Alert ── */
  .alert-error {
    display: flex;
    align-items: center;
    gap: 6px;
    background: #FEF2F2;
    border: 1px solid #FCA5A5;
    border-radius: 7px;
    padding: 8px 12px;
    font-size: 12.5px;
    color: #B91C1C;
  }

  /* ── Spec table ── */
  .table-wrap {
    overflow-x: auto;
    border: 1px solid var(--motm-border);
    border-radius: 6px;
    margin-top: 8px;
  }
  .spec-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  .spec-table th {
    background: #FAFAF9;
    text-align: left;
    padding: 7px 10px;
    font-weight: 600;
    color: var(--motm-text-muted);
    border-bottom: 1px solid var(--motm-border);
    white-space: nowrap;
  }
  .spec-table td {
    padding: 6px 10px;
    border-bottom: 1px solid var(--motm-border);
    color: var(--motm-text);
    vertical-align: top;
  }
  .spec-table tr:last-child td { border-bottom: none; }

  .badge {
    display: inline-block;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 5px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .badge-req { background: #FEE2E2; color: #B91C1C; }
  .badge-opt { background: #F3F4F6; color: #6B7280; }

  /* ── Error / warning tables ── */
  .error-table,
  .warning-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  .error-table th {
    background: #FEF2F2;
    text-align: left;
    padding: 7px 10px;
    font-weight: 600;
    color: #B91C1C;
    border-bottom: 1px solid #FECACA;
    white-space: nowrap;
  }
  .error-table td {
    padding: 6px 10px;
    border-bottom: 1px solid #FEE2E2;
    color: var(--motm-text);
    vertical-align: top;
  }
  .error-table tr:last-child td { border-bottom: none; }

  .warning-table th {
    background: #FEFCE8;
    text-align: left;
    padding: 7px 10px;
    font-weight: 600;
    color: #92400e;
    border-bottom: 1px solid #FDE68A;
    white-space: nowrap;
  }
  .warning-table td {
    padding: 6px 10px;
    border-bottom: 1px solid #FEF3C7;
    color: var(--motm-text);
    vertical-align: top;
  }
  .warning-table tr:last-child td { border-bottom: none; }

  .mono { font-family: ui-monospace, monospace; font-size: 11px; }
  .truncate { max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .suggestion { color: #16a34a; font-weight: 500; }

  /* ── Warning actions ── */
  .warning-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 10px;
    flex-wrap: wrap;
  }
  .warning-hint { font-size: 11px; color: var(--motm-text-muted); }

  /* ── Import result ── */
  .result-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
  }
  .result-item {
    background: #F0FDF4;
    border: 1px solid #BBF7D0;
    border-radius: 7px;
    padding: 10px 12px;
  }
  .result-item dt {
    font-size: 11px;
    color: #16a34a;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-bottom: 2px;
  }
  .result-item dd {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 700;
    color: #14532d;
    line-height: 1;
  }

  /* ── Template link ── */
  .template-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--motm-text-muted);
    text-decoration: none;
    transition: color 120ms;
  }
  .template-link:hover { color: var(--motm-pink); }

  /* ── Spinner ── */
  .spinner {
    width: 12px; height: 12px;
    border: 2px solid rgba(255,255,255,0.4);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .sr-only {
    position: absolute; width: 1px; height: 1px;
    padding: 0; margin: -1px; overflow: hidden;
    clip: rect(0,0,0,0); white-space: nowrap; border: 0;
  }

  code {
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
    background: rgba(0,0,0,0.06);
    padding: 0.1em 0.3em;
    border-radius: 3px;
  }
</style>

<a href="/admin/recipes" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Recipes
</a>

<div class="page-header">
  <h1 class="page-title">New recipe</h1>
  <p class="page-sub">Create a single recipe manually, or bulk-import from a spreadsheet</p>
</div>

<div class="sections">

  <!-- ── Manual creation ── -->
  <div class="card">
    <div class="card-header">
      <p class="card-title">Recipe details</p>
      <p class="card-sub">Start with a name — ingredients are managed on the next screen</p>
    </div>
    <form method="POST" action="?/create" use:enhance>
      <div class="card-body">
        <div class="field">
          <label for="name">Name</label>
          <input
            id="name"
            name="name"
            type="text"
            class:error={form?.errors?.name}
            value={form?.values?.name ?? ''}
            placeholder="e.g. Chicken Caesar Salad"
            required
          />
          {#if form?.errors?.name}
            <span class="field-error">{form.errors.name}</span>
          {/if}
        </div>
      </div>
      <div class="card-footer">
        <a href="/admin/recipes" class="btn btn-ghost">Cancel</a>
        <button type="submit" class="btn btn-primary">Create and add ingredients →</button>
      </div>
    </form>
  </div>

  <div class="divider">or</div>

  <!-- ── Excel import ── -->
  <div class="card">
    <div class="card-header">
      <div style="display: flex; justify-content: space-between; align-items: flex-start;">
        <div>
          <p class="card-title">Import from spreadsheet</p>
          <p class="card-sub">Upload an .xlsx file to bulk-import recipes and ingredients</p>
        </div>
        <a href="/admin/recipes/new/template" class="template-link" download>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
            <polyline points="7 10 12 15 17 10"/>
            <line x1="12" y1="15" x2="12" y2="3"/>
          </svg>
          Template
        </a>
      </div>
    </div>

    <div class="card-body" style="display: flex; flex-direction: column; gap: 14px;">

      <!-- Mode toggle -->
      <div class="mode-toggle">
        <label class="mode-option" class:active={mode === 'replace'}>
          <input type="radio" name="mode-radio" value="replace" bind:group={mode} />
          <strong>Replace</strong>
          <span>Overwrite ingredients for existing recipes</span>
        </label>
        <label class="mode-option" class:active={mode === 'add'}>
          <input type="radio" name="mode-radio" value="add" bind:group={mode} />
          <strong>Add new</strong>
          <span>Skip existing; warn on similar ingredients</span>
        </label>
      </div>

      <!-- Warning banner for replace mode -->
      {#if mode === 'replace'}
        <div class="notice">
          <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14" aria-hidden="true">
            <path fill-rule="evenodd" d="M8.485 2.495c.673-1.167 2.357-1.167 3.03 0l6.28 10.875c.673 1.167-.17 2.625-1.516 2.625H3.72c-1.347 0-2.189-1.458-1.515-2.625L8.485 2.495zM10 5a.75.75 0 01.75.75v3.5a.75.75 0 01-1.5 0v-3.5A.75.75 0 0110 5zm0 9a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd"/>
          </svg>
          <span><strong>Re-importing a recipe replaces its ingredient list entirely.</strong> Ingredients not in the file will be removed from that recipe.</span>
        </div>
      {/if}

      <!-- Success result -->
      {#if importResult}
        <dl class="result-grid">
          <div class="result-item">
            <dt>Rows processed</dt>
            <dd>{importResult.rows_processed}</dd>
          </div>
          <div class="result-item">
            <dt>Recipes created</dt>
            <dd>{importResult.recipes_created}</dd>
          </div>
          <div class="result-item">
            <dt>Recipes updated</dt>
            <dd>{importResult.recipes_updated}</dd>
          </div>
          <div class="result-item">
            <dt>Ingredients</dt>
            <dd>{importResult.ingredients_created}</dd>
          </div>
        </dl>
      {/if}

      <!-- Fuzzy match warnings -->
      {#if warnings?.length}
        <div>
          <p style="font-size: 12.5px; font-weight: 600; color: #92400e; margin: 0 0 6px;">
            {warnings.length} similar ingredient{warnings.length === 1 ? '' : 's'} found
          </p>
          <p style="font-size: 11.5px; color: var(--motm-text-muted); margin: 0 0 8px;">
            These names are similar to existing ones — this might indicate a typo.
          </p>
          <div class="table-wrap" style="margin-top: 0;">
            <table class="warning-table">
              <thead>
                <tr><th>Row</th><th>Your ingredient</th><th>Existing match</th><th>Score</th></tr>
              </thead>
              <tbody>
                {#each warnings as w}
                  <tr>
                    <td class="mono">{w.row}</td>
                    <td><strong>{w.raw_value}</strong></td>
                    <td class="suggestion">{w.suggestion}</td>
                    <td class="mono">{w.message.match(/\d+%/)?.[0] ?? ''}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          <div class="warning-actions">
            <button type="button" class="btn btn-primary" onclick={handleProceedAnyway}>
              Proceed anyway
            </button>
            <span class="warning-hint">Or fix the file and re-upload</span>
          </div>
        </div>
      {/if}

      <!-- Validation errors -->
      {#if parseErrors?.length}
        <div>
          <p style="font-size: 12.5px; font-weight: 600; color: #B91C1C; margin: 0 0 6px;">
            {parseErrors.length} validation {parseErrors.length === 1 ? 'error' : 'errors'} — nothing was imported
          </p>
          <div class="table-wrap" style="margin-top: 0;">
            <table class="error-table">
              <thead>
                <tr><th>Row</th><th>Field</th><th>Error</th><th>Raw value</th></tr>
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
        </div>
      {/if}

      <!-- Global import error -->
      {#if importError}
        <div class="alert-error" role="alert">
          <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14" aria-hidden="true">
            <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"/>
          </svg>
          {importError}
        </div>
      {/if}

      <!-- Upload form -->
      <form
        id="import-form"
        method="POST"
        action="?/import"
        enctype="multipart/form-data"
        use:enhance={() => {
          submitting = true;
          return async ({ update }) => { await update(); submitting = false; };
        }}
        style="display: flex; flex-direction: column; gap: 10px;"
      >
        <input type="hidden" name="mode" value={mode} />
        <input type="hidden" name="confirmed" value={confirmed ? 'true' : 'false'} />

        <label
          class="dropzone"
          class:dragging
          class:has-file={!!importFile}
          for="import-file-input"
          ondragover={(e) => { e.preventDefault(); dragging = true; }}
          ondragleave={() => (dragging = false)}
          ondrop={onDrop}
        >
          {#if importFile}
            <svg viewBox="0 0 20 20" fill="currentColor" width="22" height="22" class="drop-icon drop-icon--file" aria-hidden="true">
              <path fill-rule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4z" clip-rule="evenodd"/>
            </svg>
            <span class="drop-filename">{importFile.name}</span>
            <span class="drop-meta">{(importFile.size / 1024).toFixed(1)} KB · click to change</span>
          {:else}
            <svg viewBox="0 0 20 20" fill="currentColor" width="22" height="22" class="drop-icon" aria-hidden="true">
              <path fill-rule="evenodd" d="M3 17a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zM6.293 6.707a1 1 0 010-1.414l3-3a1 1 0 011.414 0l3 3a1 1 0 01-1.414 1.414L11 5.414V13a1 1 0 11-2 0V5.414L7.707 6.707a1 1 0 01-1.414 0z" clip-rule="evenodd"/>
            </svg>
            <span class="drop-label">Drop your <code>.xlsx</code> here, or click to browse</span>
            <span class="drop-meta">Maximum 5 MB</span>
          {/if}
          <input
            id="import-file-input"
            type="file"
            name="file"
            accept=".xlsx"
            onchange={onFileChange}
            class="sr-only"
          />
        </label>

        <details style="font-size: 12px; color: var(--motm-text-muted);">
          <summary style="cursor: pointer; user-select: none; font-weight: 600; color: var(--motm-text-muted); list-style: none; display: flex; align-items: center; gap: 4px;">
            <svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z" clip-rule="evenodd"/></svg>
            Expected column format
          </summary>
          <div class="table-wrap">
            <table class="spec-table">
              <thead>
                <tr><th>Column</th><th>Required</th><th>Notes</th></tr>
              </thead>
              <tbody>
                <tr><td>Recipe</td><td><span class="badge badge-req">Required</span></td><td>Rows with the same name form one recipe</td></tr>
                <tr><td>Ingredient</td><td><span class="badge badge-opt">Optional</span></td><td>Leave blank for a recipe with no ingredients</td></tr>
                <tr><td>Amount</td><td><span class="badge badge-opt">Optional</span></td><td>Positive decimal — required if Ingredient is set</td></tr>
                <tr><td>Unit</td><td><span class="badge badge-opt">Optional</span></td><td>kg, g, ml — required if Ingredient is set</td></tr>
                <tr><td>Allergens</td><td><span class="badge badge-opt">Optional</span></td><td>Comma-separated: <code>Dairy, Gluten</code></td></tr>
              </tbody>
            </table>
          </div>
        </details>

        <div style="display: flex; justify-content: flex-end;">
          <button type="submit" class="btn btn-primary" disabled={!importFile || submitting}>
            {#if submitting}
              <span class="spinner" aria-hidden="true"></span> Importing…
            {:else}
              Import recipes
            {/if}
          </button>
        </div>
      </form>

    </div>
  </div>

</div>
