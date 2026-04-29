<script lang="ts">
  import type { PageData } from './$types';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';

  const { data }: { data: PageData } = $props();

  const STATUS_OPTIONS = [
    { value: 'pending',   label: 'Pending' },
    { value: 'confirmed', label: 'Confirmed' },
    { value: 'fulfilled', label: 'Fulfilled' },
    { value: 'cancelled', label: 'Cancelled' },
  ];

  const SOURCE_OPTIONS = [
    { value: 'admin_manual',   label: 'Admin manual' },
    { value: 'excel_import',   label: 'Excel import' },
    { value: 'client_portal',  label: 'Client portal' },
  ];

  const statusConfig: Record<string, { label: string; color: string; bg: string }> = {
    pending:   { label: 'Pending',   color: '#92600A', bg: '#FEF3C7' },
    confirmed: { label: 'Confirmed', color: '#1D6A8F', bg: '#DBEAFE' },
    fulfilled: { label: 'Fulfilled', color: '#1F6648', bg: '#D1FAE5' },
    cancelled: { label: 'Cancelled', color: '#6B4A4A', bg: '#F3E8E8' },
  };

  // Local filter state — mirrors URL params
  let filters = $state({ ...data.filters });

  let debounceTimer: ReturnType<typeof setTimeout>;

  function applyFilters() {
    const params = new URLSearchParams();
    for (const [key, val] of Object.entries(filters)) {
      if (val !== '' && val !== null && val !== undefined) {
        params.set(key, String(val));
      }
    }
    goto(`?${params.toString()}`, { replaceState: true });
  }

  function debouncedApply() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(applyFilters, 350);
  }

  function clearFilters() {
    filters = {
      status: '', company_id: '', serving_from: '', serving_to: '',
      shipment_from: '', shipment_to: '', order_source: '', frozen: '', chilled: '',
    };
    goto('?', { replaceState: true });
  }

  const hasActiveFilters = $derived(
    Object.values(filters).some(v => v !== '' && v !== null && v !== undefined)
  );

  function formatDate(val: string | Date): string {
    return new Date(val).toLocaleDateString('en-GB', {
      day: 'numeric', month: 'short', year: 'numeric',
    });
  }
</script>

<style>
  .page-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 24px;
    gap: 16px;
    flex-wrap: wrap;
  }

  /* Layout */
  .layout {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 20px;
    align-items: start;
  }
  @media (max-width: 860px) {
    .layout { grid-template-columns: 1fr; }
    .sidebar { display: none; }
  }

  /* Sidebar */
  .sidebar {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    overflow: hidden;
    position: sticky;
    top: 72px;
  }
  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px 12px;
    border-bottom: 1px solid var(--motm-border);
  }
  .sidebar-title {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--motm-text-muted);
  }
  .clear-btn {
    font-size: 12px;
    color: var(--motm-pink);
    background: transparent;
    border: none;
    cursor: pointer;
    font-family: inherit;
    font-weight: 500;
    padding: 0;
    transition: color 120ms;
  }
  .clear-btn:hover { color: var(--motm-pink-dark); }
  .clear-btn:disabled { color: var(--motm-text-muted); cursor: default; }

  .filter-group {
    padding: 14px 16px;
    border-bottom: 1px solid var(--motm-border);
  }
  .filter-group:last-child { border-bottom: none; }
  .filter-label {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--motm-text);
    margin-bottom: 8px;
    display: block;
  }

  select, input[type="date"] {
    width: 100%;
    padding: 7px 9px;
    border: 1px solid var(--motm-border);
    border-radius: 7px;
    font-size: 12.5px;
    font-family: inherit;
    color: var(--motm-text);
    background: #fff;
    outline: none;
    transition: border-color 120ms;
    box-sizing: border-box;
  }
  select:focus, input[type="date"]:focus { border-color: var(--motm-pink); }

  .date-range { display: flex; flex-direction: column; gap: 6px; }
  .date-range-label { font-size: 11px; color: var(--motm-text-muted); margin-bottom: 2px; }

  /* Toggle pills for frozen/chilled */
  .toggle-group { display: flex; gap: 6px; flex-wrap: wrap; }
  .toggle-pill {
    padding: 4px 10px;
    border-radius: 20px;
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: 1.5px solid var(--motm-border);
    background: #fff;
    color: var(--motm-text-muted);
    transition: all 120ms;
  }
  .toggle-pill:hover { border-color: var(--motm-pink); color: var(--motm-text); }
  .toggle-pill.active {
    background: var(--motm-pink-light);
    border-color: var(--motm-pink);
    color: var(--motm-pink-dark);
  }

  /* Main content */
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 16px;
    flex-wrap: wrap;
  }
  .result-count {
    font-size: 12.5px;
    color: var(--motm-text-muted);
    flex: 1;
  }
  .import-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    background: var(--motm-pink);
    color: #fff;
    border: none;
    cursor: pointer;
    text-decoration: none;
    transition: background 120ms;
    white-space: nowrap;
  }
  .import-btn:hover { background: var(--motm-pink-dark); }

  /* Table */
  .table-wrap {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    overflow: hidden;
  }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  thead tr { border-bottom: 1px solid var(--motm-border); }
  th {
    text-align: left;
    padding: 10px 14px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--motm-text-muted);
    white-space: nowrap;
    background: #FAF8F6;
  }
  td {
    padding: 11px 14px;
    color: var(--motm-text);
    border-bottom: 1px solid var(--motm-border);
    vertical-align: middle;
  }
  tbody tr:last-child td { border-bottom: none; }
  tbody tr:hover td { background: #FAF8F6; }
  .td-muted { color: var(--motm-text-muted); font-size: 12.5px; }

  .status-badge {
    display: inline-flex;
    padding: 2px 8px;
    border-radius: 5px;
    font-size: 11.5px;
    font-weight: 600;
    white-space: nowrap;
  }
  .source-badge {
    display: inline-flex;
    padding: 2px 7px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 500;
    background: #F3F3F1;
    color: #666;
    white-space: nowrap;
  }
  .flag-dot {
    display: inline-block;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    margin-right: 3px;
  }
  .empty-state {
    padding: 56px;
    text-align: center;
    color: var(--motm-text-muted);
    font-size: 13.5px;
  }
  .empty-title { font-weight: 500; color: var(--motm-text); margin-bottom: 4px; }
</style>

<div class="page-header">
  <div>
    <h1 style="font-size: 22px; font-weight: 600; color: var(--motm-text); letter-spacing: -0.02em;">Orders</h1>
    <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">View and manage all delivery orders</p>
  </div>
</div>

<div class="layout">
  <!-- Filter sidebar -->
  <aside class="sidebar">
    <div class="sidebar-header">
      <span class="sidebar-title">Filters</span>
      <button class="clear-btn" onclick={clearFilters} disabled={!hasActiveFilters}>
        Clear all
      </button>
    </div>

    <!-- Status -->
    <div class="filter-group">
      <span class="filter-label">Status</span>
      <select
        bind:value={filters.status}
        onchange={applyFilters}
      >
        <option value="">All statuses</option>
        {#each STATUS_OPTIONS as opt}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>

    <!-- Company -->
    <div class="filter-group">
      <span class="filter-label">Company</span>
      <select
        bind:value={filters.company_id}
        onchange={applyFilters}
      >
        <option value="">All companies</option>
        {#each data.companies as company}
          <option value={company.id}>{company.name}</option>
        {/each}
      </select>
    </div>

    <!-- Serving date range -->
    <div class="filter-group">
      <span class="filter-label">Serving date</span>
      <div class="date-range">
        <div>
          <p class="date-range-label">From</p>
          <input
            type="date"
            bind:value={filters.serving_from}
            onchange={applyFilters}
          />
        </div>
        <div>
          <p class="date-range-label">To</p>
          <input
            type="date"
            bind:value={filters.serving_to}
            onchange={applyFilters}
          />
        </div>
      </div>
    </div>

    <!-- Shipment date range -->
    <div class="filter-group">
      <span class="filter-label">Shipment date</span>
      <div class="date-range">
        <div>
          <p class="date-range-label">From</p>
          <input
            type="date"
            bind:value={filters.shipment_from}
            onchange={applyFilters}
          />
        </div>
        <div>
          <p class="date-range-label">To</p>
          <input
            type="date"
            bind:value={filters.shipment_to}
            onchange={applyFilters}
          />
        </div>
      </div>
    </div>

    <!-- Order source -->
    <div class="filter-group">
      <span class="filter-label">Source</span>
      <select
        bind:value={filters.order_source}
        onchange={applyFilters}
      >
        <option value="">All sources</option>
        {#each SOURCE_OPTIONS as opt}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>

    <!-- Frozen / Chilled -->
    <div class="filter-group">
      <span class="filter-label">Temperature</span>
      <div class="toggle-group">
        <button
          type="button"
          class="toggle-pill"
          class:active={filters.frozen === 'true'}
          onclick={() => {
            filters.frozen = filters.frozen === 'true' ? '' : 'true';
            filters.chilled = '';
            applyFilters();
          }}
        >
          ❄ Frozen
        </button>
        <button
          type="button"
          class="toggle-pill"
          class:active={filters.chilled === 'true'}
          onclick={() => {
            filters.chilled = filters.chilled === 'true' ? '' : 'true';
            filters.frozen = '';
            applyFilters();
          }}
        >
          🧊 Chilled
        </button>
      </div>
    </div>
  </aside>

  <!-- Main table -->
  <div>
    <div class="toolbar">
      <span class="result-count">
        {data.orders.length} {data.orders.length === 1 ? 'order' : 'orders'}
        {#if hasActiveFilters} (filtered){/if}
      </span>
      <a href="/admin/orders/import" class="import-btn">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
          <polyline points="17 8 12 3 7 8"/>
          <line x1="12" y1="3" x2="12" y2="15"/>
        </svg>
        Import orders
      </a>
    </div>

    <div class="table-wrap">
      {#if data.orders.length === 0}
        <div class="empty-state">
          <p class="empty-title">No orders found</p>
          <p>{hasActiveFilters ? 'Try adjusting your filters.' : 'No orders have been created yet.'}</p>
        </div>
      {:else}
        <table>
          <thead>
            <tr>
              <th>Company</th>
              <th>Recipe</th>
              <th>Serving date</th>
              <th>Shipment date</th>
              <th>Amount</th>
              <th>Source</th>
              <th>Status</th>
              <th>Flags</th>
            </tr>
          </thead>
          <tbody>
            {#each data.orders as order}
              {@const status = statusConfig[order.status]}
              <tr
                onclick={() => goto(`/admin/orders/${order.id}`)}
                style="cursor: pointer;"
              >
                <td style="font-weight: 500;">{order.company_name}</td>
                <td class="td-muted">{order.recipe_name}</td>
                <td class="td-muted">{formatDate(order.serving_date)}</td>
                <td class="td-muted">{order.shipment_date ? formatDate(order.shipment_date) : '—'}</td>
                <td class="td-muted">{order.amount}</td>
                <td>
                  <span class="source-badge">
                    {order.order_source.replace(/_/g, ' ')}
                  </span>
                </td>
                <td>
                  <span
                    class="status-badge"
                    style="background: {status.bg}; color: {status.color};"
                  >
                    {status.label}
                  </span>
                </td>
                <td class="td-muted">
                  {#if order.frozen}
                    <span class="flag-dot" style="background: #93C5FD;"></span>Frozen
                  {:else if order.chilled}
                    <span class="flag-dot" style="background: #6EE7B7;"></span>Chilled
                  {:else}
                    —
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </div>
</div>
