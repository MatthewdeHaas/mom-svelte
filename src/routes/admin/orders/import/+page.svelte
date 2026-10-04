<script lang="ts">
  import type { ActionData } from './$types';
  import type { ImportWarning } from '$lib/types';
  import { enhance } from '$app/forms';

  const { form }: { form: ActionData } = $props();

  // Group row-level warnings by code so the UI collapses to one line per code
  // with the row numbers listed. Sheet-level warnings (row === null) render
  // one per line unchanged.
  type GroupedWarning = {
    code: string;
    message: string;
    rows: number[];      // empty for sheet-level
    sheetLevel: boolean;
  };

  function groupWarnings(warnings: ImportWarning[] | undefined): GroupedWarning[] {
    if (!warnings?.length) return [];
    const sheetLevel: GroupedWarning[] = [];
    const byCode = new Map<string, GroupedWarning>();
    for (const w of warnings) {
      if (w.row === null) {
        sheetLevel.push({ code: w.code, message: w.message, rows: [], sheetLevel: true });
        continue;
      }
      const existing = byCode.get(w.code);
      if (existing) {
        existing.rows.push(w.row);
      } else {
        byCode.set(w.code, {
          code: w.code,
          message: w.message,
          rows: [w.row],
          sheetLevel: false,
        });
      }
    }
    return [
      ...sheetLevel,
      ...[...byCode.values()].map((g) => ({ ...g, rows: [...g.rows].sort((a, b) => a - b) })),
    ];
  }

  const groupedWarnings = $derived(groupWarnings(form?.warnings ?? form?.importWarnings));

  let fileInput = $state<HTMLInputElement | null>(null);
  let selectedFile = $state<File | null>(null);
  let dragging = $state(false);

  function onFileChange(e: Event) {
    const input = e.target as HTMLInputElement;
    selectedFile = input.files?.[0] ?? null;
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    const file = e.dataTransfer?.files?.[0];
    if (file && fileInput) {
      const dt = new DataTransfer();
      dt.items.add(file);
      fileInput.files = dt.files;
      selectedFile = file;
    }
  }

  function onDragover(e: DragEvent) {
    e.preventDefault();
    dragging = true;
  }

  function onDragleave() { dragging = false; }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  // Reset file after successful import
  $effect(() => {
    if (form?.success && fileInput) {
      fileInput.value = '';
      selectedFile = null;
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

  .layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
    align-items: start;
  }
  @media (max-width: 780px) { .layout { grid-template-columns: 1fr; } }

  .card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    margin-bottom: 20px;
  }
  .card-header { padding: 18px 22px 14px; border-bottom: 1px solid var(--motm-border); }
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

  /* Drop zone */
  .drop-zone {
    border: 2px dashed var(--motm-border);
    border-radius: 10px;
    padding: 36px 24px;
    text-align: center;
    cursor: pointer;
    transition: border-color 150ms, background 150ms;
    position: relative;
  }
  .drop-zone:hover, .drop-zone.dragging {
    border-color: var(--motm-pink);
    background: #FDF5F7;
  }
  .drop-zone input[type="file"] {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
    width: 100%;
    height: 100%;
  }
  .drop-icon {
    width: 36px;
    height: 36px;
    color: var(--motm-pink);
    margin: 0 auto 12px;
  }
  .drop-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--motm-text);
    margin-bottom: 4px;
  }
  .drop-sub { font-size: 12.5px; color: var(--motm-text-muted); }

  /* Selected file pill */
  .file-pill {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: var(--motm-pink-light);
    border: 1px solid #EAC4D0;
    border-radius: 8px;
    margin-top: 14px;
  }
  .file-pill-name { font-size: 13px; font-weight: 500; color: var(--motm-pink-dark); flex: 1; }
  .file-pill-size { font-size: 12px; color: var(--motm-pink-dark); opacity: 0.7; }

  /* Result states */
  .success-box {
    border: 1px solid #A7F3D0;
    border-radius: 10px;
    overflow: hidden;
  }
  .success-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 20px;
    background: #D1FAE5;
    border-bottom: 1px solid #A7F3D0;
  }
  .success-title { font-size: 14px; font-weight: 600; color: #1F6648; }
  .success-stats {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    background: #A7F3D0;
  }
  .stat-cell {
    background: #F0FDF4;
    padding: 18px 20px;
    text-align: center;
  }
  .stat-value { font-size: 28px; font-weight: 600; color: #1F6648; letter-spacing: -0.03em; }
  .stat-label { font-size: 12px; color: #3D8460; margin-top: 2px; }

  .error-box {
    border: 1px solid #F0CBCB;
    border-radius: 10px;
    overflow: hidden;
    margin-bottom: 20px;
  }
  .error-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 20px;
    background: #FCEBEB;
    border-bottom: 1px solid #F0CBCB;
  }
  .error-title { font-size: 14px; font-weight: 600; color: #A32D2D; }
  .error-sub { font-size: 12.5px; color: #C05050; margin-top: 1px; }

  /* Warnings strip — non-fatal normalization notices */
  .warnings-box {
    border: 1px solid #F5D679;
    border-radius: 10px;
    overflow: hidden;
    margin-bottom: 20px;
  }
  .warnings-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 20px;
    background: #FFF8E1;
    border-bottom: 1px solid #F5D679;
  }
  .warnings-title { font-size: 14px; font-weight: 600; color: #8B6508; }
  .warnings-sub   { font-size: 12.5px; color: #A17A1A; margin-top: 1px; }
  .warnings-body { padding: 10px 0; background: #FFFDF4; }
  .warning-row {
    padding: 8px 20px;
    font-size: 12.5px;
    color: #5A4710;
    line-height: 1.5;
  }
  .warning-row + .warning-row { border-top: 1px solid #FBECBD; }
  .warning-row .code {
    display: inline-block;
    font-family: monospace;
    font-size: 11px;
    color: #8B6508;
    background: #FBECBD;
    padding: 1px 6px;
    border-radius: 3px;
    margin-right: 6px;
  }
  .warning-row .rows {
    color: #A17A1A;
    font-size: 11.5px;
    margin-left: 6px;
  }

  /* Plain error (bad file) */
  .plain-error {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 14px 16px;
    background: #FCEBEB;
    border: 1px solid #F0CBCB;
    border-radius: 8px;
    font-size: 13px;
    color: #A32D2D;
    line-height: 1.5;
    margin-bottom: 20px;
  }

  /* Error table */
  .error-table-wrap { overflow-x: auto; max-height: 420px; overflow-y: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
  thead { position: sticky; top: 0; z-index: 1; }
  th {
    text-align: left;
    padding: 9px 14px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: #A32D2D;
    background: #FEF0F0;
    border-bottom: 1px solid #F0CBCB;
    white-space: nowrap;
  }
  td {
    padding: 9px 14px;
    border-bottom: 1px solid #FDE8E8;
    color: var(--motm-text);
    vertical-align: top;
  }
  tbody tr:last-child td { border-bottom: none; }
  tbody tr:hover td { background: #FFF5F5; }
  .td-muted { color: var(--motm-text-muted); }
  .td-field { font-family: monospace; font-size: 12px; color: #A32D2D; }

  /* Buttons */
  .btn {
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    text-decoration: none;
  }
  .btn-primary { background: var(--motm-pink); color: #fff; }
  .btn-primary:hover { background: var(--motm-pink-dark); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-ghost {
    background: transparent;
    color: var(--motm-text-muted);
    border: 1px solid var(--motm-border);
  }
  .btn-ghost:hover { background: #FAF8F6; color: var(--motm-text); }
</style>

<a href="/admin/orders" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Orders
</a>

<div style="margin-bottom: 28px;">
  <h1 style="font-size: 22px; font-weight: 600; color: var(--motm-text); letter-spacing: -0.02em;">Import orders</h1>
  <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">Upload an Excel file to bulk-create orders</p>
</div>

<!-- Plain error (bad file / server error) -->
{#if form?.error}
  <div class="plain-error">
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="flex-shrink:0; margin-top: 1px;">
      <circle cx="12" cy="12" r="10"/><path d="M12 8v4m0 4h.01"/>
    </svg>
    {form.error}
  </div>
{/if}

<!-- Warnings strip — non-fatal normalization notices -->
{#if groupedWarnings.length}
  <div class="warnings-box">
    <div class="warnings-header">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color: #8B6508;">
        <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
        <line x1="12" y1="9"  x2="12" y2="13"/>
        <line x1="12" y1="17" x2="12.01" y2="17"/>
      </svg>
      <div>
        <p class="warnings-title">{groupedWarnings.length} normalization {groupedWarnings.length === 1 ? 'notice' : 'notices'}</p>
        <p class="warnings-sub">The importer accepted the file but made the adjustments below</p>
      </div>
    </div>
    <div class="warnings-body">
      {#each groupedWarnings as w}
        <div class="warning-row">
          <span class="code">{w.code}</span>
          {w.message}
          {#if !w.sheetLevel && w.rows.length}
            <span class="rows">
              (row{w.rows.length === 1 ? '' : 's'} {w.rows.length > 20
                ? `${w.rows.slice(0, 20).join(', ')}, … +${w.rows.length - 20} more`
                : w.rows.join(', ')})
            </span>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<!-- Row-level validation errors -->
{#if form?.importErrors?.length}
  <div class="error-box">
    <div class="error-header">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"/><path d="M12 8v4m0 4h.01"/>
      </svg>
      <div>
        <p class="error-title">{form.importErrors.length} {form.importErrors.length === 1 ? 'error' : 'errors'} found</p>
        <p class="error-sub">Fix the issues below and re-upload the file</p>
      </div>
    </div>
    <div class="error-table-wrap">
      <table>
        <thead>
          <tr>
            <th>Row</th>
            <th>Field</th>
            <th>Error</th>
          </tr>
        </thead>
        <tbody>
          {#each form.importErrors as err}
            <tr>
              <td style="font-weight: 600; color: #A32D2D;">{err.row}</td>
              <td>
                {#if err.field}
                  <span class="td-field">{err.field}</span>
                {:else}
                  <span class="td-muted">—</span>
                {/if}
              </td>
              <td>{err.message}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
{/if}

<!-- Success result -->
{#if form?.success}
  <div class="success-box" style="margin-bottom: 24px;">
    <div class="success-header">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
        <path d="M20 6 9 17l-5-5"/>
      </svg>
      <p class="success-title">Import complete</p>
    </div>
    <div class="success-stats">
      <div class="stat-cell">
        <p class="stat-value">{form.inserted}</p>
        <p class="stat-label">Orders inserted</p>
      </div>
      <div class="stat-cell">
        <p class="stat-value">{form.skipped}</p>
        <p class="stat-label">Rows skipped</p>
      </div>
    </div>
  </div>
{/if}

<div class="layout">
  <!-- Upload form -->
  <div class="card">
    <div class="card-header">
      <p class="card-title">Upload file</p>
      <p class="card-sub">Accepted format: .xlsx, max 5MB</p>
    </div>
    <form method="POST" enctype="multipart/form-data" use:enhance>
      <div class="card-body">
        <div
          class="drop-zone"
          class:dragging
          ondrop={onDrop}
          ondragover={onDragover}
          ondragleave={onDragleave}
          role="presentation"
        >
          <input
            type="file"
            name="file"
            accept=".xlsx"
            bind:this={fileInput}
            onchange={onFileChange}
          />
          <svg class="drop-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
            <polyline points="17 8 12 3 7 8"/>
            <line x1="12" y1="3" x2="12" y2="15"/>
          </svg>
          <p class="drop-title">
            {dragging ? 'Drop to upload' : 'Drag & drop or click to browse'}
          </p>
          <p class="drop-sub">.xlsx files only</p>
        </div>

        {#if selectedFile}
          <div class="file-pill">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color: var(--motm-pink-dark); flex-shrink: 0;">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>
            </svg>
            <span class="file-pill-name">{selectedFile.name}</span>
            <span class="file-pill-size">{formatBytes(selectedFile.size)}</span>
          </div>
        {/if}
      </div>

      <div class="card-footer">
        <a href="/admin/orders" class="btn btn-ghost">Cancel</a>
        <button type="submit" class="btn btn-primary" disabled={!selectedFile}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
            <polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>
          </svg>
          Import
        </button>
      </div>
    </form>
  </div>

  <!-- Format guide -->
  <div class="card">
    <div class="card-header" style="display: flex; align-items: flex-start; justify-content: space-between; gap: 12px;">
      <div>
        <p class="card-title">Expected format</p>
        <p class="card-sub">Column headers must match exactly</p>
      </div>
      <a href="/admin/orders/import/template" class="btn btn-ghost" style="flex-shrink: 0; font-size: 12.5px; padding: 6px 12px;">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
          <polyline points="7 10 12 15 17 10"/>
          <line x1="12" y1="15" x2="12" y2="3"/>
        </svg>
        Download template
      </a>
    </div>
    <div class="card-body">
      <table style="width: 100%; border-collapse: collapse; font-size: 12.5px;">
        <thead>
          <tr>
            <th style="text-align: left; padding: 7px 10px; font-size: 10.5px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--motm-text-muted); border-bottom: 1px solid var(--motm-border);">Column</th>
            <th style="text-align: left; padding: 7px 10px; font-size: 10.5px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--motm-text-muted); border-bottom: 1px solid var(--motm-border);">Required</th>
            <th style="text-align: left; padding: 7px 10px; font-size: 10.5px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--motm-text-muted); border-bottom: 1px solid var(--motm-border);">Notes</th>
          </tr>
        </thead>
        <tbody>
          {#each [
            { col: 'Customer',      req: true,  note: 'Must match existing company name' },
            { col: 'Recipe',        req: true,  note: 'Must match existing recipe name' },
            { col: 'Serving Date',  req: true,  note: 'YYYY-MM-DD' },
            { col: 'Order Amount',  req: true,  note: 'Positive number' },
            { col: 'Meal',          req: true,  note: 'Breakfast, Lunch, Snack, Snack R, or Dinner' },
            { col: 'Shipment Date', req: true,  note: 'YYYY-MM-DD, ≤ serving date' },
            { col: 'Portion Size',  req: false, note: 'Positive number (grams/units per serving)' },
            { col: 'Day',           req: false, note: '1-7 (production cycle day)' },
            { col: 'Storage',       req: false, note: 'f = frozen, c = chilled (blank = ambient)' },
            { col: 'Client',        req: false, note: 'Client name and/or allergens' },
          ] as row}
            <tr>
              <td style="padding: 7px 10px; font-family: monospace; font-size: 12px; border-bottom: 1px solid var(--motm-border); color: var(--motm-text);">{row.col}</td>
              <td style="padding: 7px 10px; border-bottom: 1px solid var(--motm-border);">
                {#if row.req}
                  <span style="font-size: 11px; font-weight: 600; color: #A32D2D; background: #FCEBEB; padding: 1px 6px; border-radius: 3px;">Yes</span>
                {:else}
                  <span style="font-size: 11px; color: var(--motm-text-muted);">No</span>
                {/if}
              </td>
              <td style="padding: 7px 10px; border-bottom: 1px solid var(--motm-border); color: var(--motm-text-muted); font-size: 12px;">{row.note}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
</div>
