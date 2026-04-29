<script lang="ts">
  // Export hrefs — wire these up once the backend routes are built.
  // See TODO.md for the planned route structure.
  const EXPORTS = [
    {
      id: 'shopping-list',
      title: 'Shopping list',
      description: 'Consolidated ingredient quantities across all confirmed orders for a given date range. Used for purchasing.',
      format: 'Excel (.xlsx)',
      icon: 'cart',
      // TODO: replace with real route e.g. /admin/exports/shopping-list?from=...&to=...
      href: null,
    },
    {
      id: 'kitchen-prep',
      title: 'Kitchen prep list',
      description: 'Per-recipe production quantities broken down by meal category and serving date. Used by the kitchen team.',
      format: 'Excel (.xlsx)',
      icon: 'utensils',
      // TODO: replace with real route e.g. /admin/exports/kitchen-prep?date=...
      href: null,
    },
    {
      id: 'packing-slips',
      title: 'Packing slips',
      description: 'Per-company packing slips for a given shipment date, ready for printing and dispatch.',
      format: 'PDF',
      icon: 'document',
      // TODO: replace with real route e.g. /admin/exports/packing-slips?shipment_date=...
      href: null,
    },
  ] as const;
</script>

<style>
  .page-header { margin-bottom: 28px; }
  .page-title {
    font-size: 22px;
    font-weight: 600;
    color: var(--motm-text);
    letter-spacing: -0.02em;
  }
  .page-sub {
    font-size: 13.5px;
    color: var(--motm-text-muted);
    margin-top: 3px;
  }

  .notice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 13px 16px;
    background: #FEF9EC;
    border: 1px solid #F5D97A;
    border-radius: 8px;
    font-size: 13px;
    color: #7A5800;
    line-height: 1.55;
    margin-bottom: 28px;
  }
  .notice svg { flex-shrink: 0; margin-top: 1px; }

  .export-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 16px;
  }

  .export-card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .export-card-body {
    padding: 22px 22px 18px;
    flex: 1;
  }

  .export-icon {
    width: 38px;
    height: 38px;
    border-radius: 9px;
    background: var(--motm-pink-light);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 14px;
  }
  .export-icon svg {
    width: 18px;
    height: 18px;
    color: var(--motm-pink-dark);
  }

  .export-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--motm-text);
    margin-bottom: 6px;
  }
  .export-desc {
    font-size: 13px;
    color: var(--motm-text-muted);
    line-height: 1.55;
    margin-bottom: 14px;
  }
  .export-format {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 4px;
    background: #F3F3F1;
    color: #666;
  }

  .export-card-footer {
    padding: 14px 22px;
    border-top: 1px solid var(--motm-border);
    background: #FAF8F6;
  }

  /* Download button — active */
  .download-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    width: 100%;
    padding: 9px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    background: var(--motm-pink);
    color: #fff;
    border: none;
    cursor: pointer;
    text-decoration: none;
    transition: background 120ms;
  }
  .download-btn:hover { background: var(--motm-pink-dark); }

  /* Stub button — not yet available */
  .stub-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    width: 100%;
    padding: 9px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    background: transparent;
    color: var(--motm-text-muted);
    border: 1.5px dashed var(--motm-border);
    cursor: not-allowed;
  }
  .stub-label {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--motm-text-muted);
    opacity: 0.7;
    margin-left: 4px;
  }
</style>

<div class="page-header">
  <h1 class="page-title">Exports</h1>
  <p class="page-sub">Download production and logistics documents</p>
</div>

<div class="notice">
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <circle cx="12" cy="12" r="10"/><path d="M12 8v4m0 4h.01"/>
  </svg>
  Export endpoints are not yet built. The buttons below are placeholders — wire up the <code style="font-family: monospace; font-size: 12px; background: rgba(0,0,0,0.06); padding: 1px 5px; border-radius: 3px;">href</code> values in <code style="font-family: monospace; font-size: 12px; background: rgba(0,0,0,0.06); padding: 1px 5px; border-radius: 3px;">+page.svelte</code> once the routes exist.
</div>

<div class="export-grid">
  {#each EXPORTS as exp}
    <div class="export-card">
      <div class="export-card-body">
        <div class="export-icon">
          {#if exp.icon === 'cart'}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
              <path d="M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4z"/>
              <line x1="3" y1="6" x2="21" y2="6"/>
              <path d="M16 10a4 4 0 0 1-8 0"/>
            </svg>
          {:else if exp.icon === 'utensils'}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
              <path d="M3 2v7c0 1.1.9 2 2 2h4a2 2 0 0 0 2-2V2"/>
              <path d="M7 2v20"/>
              <path d="M21 15V2a5 5 0 0 0-5 5v6c0 1.1.9 2 2 2h3zm0 0v7"/>
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
              <polyline points="14 2 14 8 20 8"/>
              <line x1="16" y1="13" x2="8" y2="13"/>
              <line x1="16" y1="17" x2="8" y2="17"/>
              <polyline points="10 9 9 9 8 9"/>
            </svg>
          {/if}
        </div>

        <p class="export-title">{exp.title}</p>
        <p class="export-desc">{exp.description}</p>

        <span class="export-format">
          {#if exp.format.includes('xlsx')}
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <polyline points="8 17 12 21 16 17"/>
              <line x1="12" y1="12" x2="12" y2="21"/>
              <path d="M20.88 18.09A5 5 0 0 0 18 9h-1.26A8 8 0 1 0 3 16.29"/>
            </svg>
          {:else}
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
              <polyline points="14 2 14 8 20 8"/>
            </svg>
          {/if}
          {exp.format}
        </span>
      </div>

      <div class="export-card-footer">
        {#if exp.href}
          <a href={exp.href} download class="download-btn">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <polyline points="8 17 12 21 16 17"/>
              <line x1="12" y1="12" x2="12" y2="21"/>
              <path d="M20.88 18.09A5 5 0 0 0 18 9h-1.26A8 8 0 1 0 3 16.29"/>
            </svg>
            Download
          </a>
        {:else}
          <div class="stub-btn">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
              <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
            </svg>
            Download
            <span class="stub-label">Not yet available</span>
          </div>
        {/if}
      </div>
    </div>
  {/each}
</div>
