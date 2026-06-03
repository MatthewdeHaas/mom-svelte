<script lang="ts">
  import type { PageData, ActionData } from './$types';
  import { enhance } from '$app/forms';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  const MEAL_CATEGORIES = ['Breakfast', 'Lunch', 'Snack', 'Dinner'];

  const statusConfig: Record<string, { label: string; color: string; bg: string }> = {
    pending:   { label: 'Pending',   color: '#92600A', bg: '#FEF3C7' },
    confirmed: { label: 'Confirmed', color: '#1D6A8F', bg: '#DBEAFE' },
    fulfilled: { label: 'Fulfilled', color: '#1F6648', bg: '#D1FAE5' },
    cancelled: { label: 'Cancelled', color: '#6B4A4A', bg: '#F3E8E8' },
  };

  const isTerminal = $derived(
    data.order.status === 'fulfilled' || data.order.status === 'cancelled'
  );

  const canConfirm  = $derived(data.order.status === 'pending');
  const canFulfill  = $derived(data.order.status === 'confirmed');
  const canCancel   = $derived(['pending', 'confirmed'].includes(data.order.status));

  // Frozen/chilled are mutually exclusive — manage as a single selection
  let tempMode = $state('none');

  $effect(() => {
    tempMode = data.order.frozen ? 'frozen' : data.order.chilled ? 'chilled' : 'none';
  });

  function formatDate(val: string | Date | null): string {
    if (!val) return '—';
    return new Date(val).toLocaleDateString('en-GB', {
      day: 'numeric', month: 'short', year: 'numeric',
    });
  }

  function toInputDate(val: string | Date | null): string {
    if (!val) return '';
    return new Date(val).toISOString().slice(0, 10);
  }

  // Re-sync tempMode after a successful update (data re-loads)
  $effect(() => {
    tempMode = data.order.frozen ? 'frozen' : data.order.chilled ? 'chilled' : 'none';
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
  }

  .status-badge {
    display: inline-flex;
    padding: 3px 10px;
    border-radius: 5px;
    font-size: 12.5px;
    font-weight: 600;
  }

  /* Layout */
  .layout {
    display: grid;
    grid-template-columns: 1fr 280px;
    gap: 20px;
    align-items: start;
  }
  @media (max-width: 860px) { .layout { grid-template-columns: 1fr; } }

  /* Cards */
  .card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    margin-bottom: 20px;
  }
  .card-header { padding: 18px 22px 14px; border-bottom: 1px solid var(--motm-border); }
  .card-title { font-size: 14px; font-weight: 600; color: var(--motm-text); }
  .card-sub { font-size: 12.5px; color: var(--motm-text-muted); margin-top: 2px; }
  .card-body { padding: 22px; display: flex; flex-direction: column; gap: 16px; }
  .card-footer {
    padding: 14px 22px;
    border-top: 1px solid var(--motm-border);
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    align-items: center;
  }

  /* Fields */
  .field-row { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .field { display: flex; flex-direction: column; gap: 5px; }
  label, .field-label { font-size: 12.5px; font-weight: 600; color: var(--motm-text); }
  .field-error { font-size: 12px; color: #A32D2D; }
  .field-hint { font-size: 12px; color: var(--motm-text-muted); }

  input[type="number"], input[type="date"], select, textarea {
    padding: 8px 11px;
    border: 1px solid var(--motm-border);
    border-radius: 7px;
    font-size: 13.5px;
    font-family: inherit;
    color: var(--motm-text);
    background: #fff;
    outline: none;
    transition: border-color 120ms;
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
  }
  input:focus, select:focus, textarea:focus { border-color: var(--motm-pink); }
  input.error, select.error { border-color: #E24B4A; }
  input:disabled, select:disabled, textarea:disabled {
    background: #FAF8F6;
    color: var(--motm-text-muted);
    cursor: not-allowed;
  }

  /* Temp mode pills */
  .temp-pills { display: flex; gap: 8px; }
  .temp-pill {
    padding: 6px 14px;
    border-radius: 20px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: 1.5px solid var(--motm-border);
    background: #fff;
    color: var(--motm-text-muted);
    transition: all 120ms;
  }
  .temp-pill:hover:not(:disabled) { border-color: var(--motm-pink); color: var(--motm-text); }
  .temp-pill.active {
    background: var(--motm-pink-light);
    border-color: var(--motm-pink);
    color: var(--motm-pink-dark);
  }
  .temp-pill:disabled { opacity: 0.5; cursor: not-allowed; }

  /* Read-only field display */
  .ro-field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .ro-label { font-size: 11.5px; font-weight: 600; color: var(--motm-text-muted); letter-spacing: 0.04em; text-transform: uppercase; }
  .ro-value { font-size: 13.5px; color: var(--motm-text); font-weight: 500; }
  .ro-value.muted { color: var(--motm-text-muted); font-weight: 400; }

  .meta-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px 20px;
  }

  /* Status transitions */
  .transitions-card { background: #fff; border: 1px solid var(--motm-border); border-radius: 10px; overflow: hidden; margin-bottom: 16px; }
  .transitions-header { padding: 14px 18px 12px; border-bottom: 1px solid var(--motm-border); }
  .transitions-body { padding: 16px 18px; display: flex; flex-direction: column; gap: 8px; }

  .transition-btn {
    width: 100%;
    padding: 10px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    transition: background 120ms, opacity 120ms;
    text-align: left;
  }
  .transition-btn .arrow { opacity: 0.6; font-size: 14px; }
  .btn-confirm { background: #DBEAFE; color: #1D6A8F; }
  .btn-confirm:hover { background: #BFDBFE; }
  .btn-fulfill { background: #D1FAE5; color: #1F6648; }
  .btn-fulfill:hover { background: #A7F3D0; }
  .btn-cancel { background: #FCEBEB; color: #A32D2D; }
  .btn-cancel:hover { background: #FEE2E2; }
  .transition-btn:disabled { opacity: 0.35; cursor: not-allowed; }

  .terminal-notice {
    padding: 14px 18px;
    font-size: 13px;
    color: var(--motm-text-muted);
    line-height: 1.5;
  }

  /* Source badge */
  .source-chip {
    display: inline-flex;
    padding: 3px 9px;
    border-radius: 4px;
    font-size: 11.5px;
    font-weight: 500;
    background: #F3F3F1;
    color: #666;
  }

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
  .strip-info { background: #DBEAFE; color: #1D6A8F; }
  .strip-warning { background: #FEF3C7; color: #92600A; }

  /* Buttons */
  .btn {
    padding: 7px 14px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    text-decoration: none;
  }
  .btn-primary { background: var(--motm-pink); color: #fff; }
  .btn-primary:hover { background: var(--motm-pink-dark); }
</style>

<a href="/admin/orders" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Orders
</a>

<div class="page-header">
  <div>
    <h1 class="page-title">{data.order.company.name}</h1>
    <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 4px;">
      {data.order.recipe?.name} · {formatDate(data.order.serving_date)}
    </p>
  </div>
  <span class="status-badge" style="background: {statusConfig[data.order.status].bg}; color: {statusConfig[data.order.status].color};">{statusConfig[data.order.status].label}</span>
</div>

<!-- Action strips -->
{#if form?.action === 'update' && form?.success}
  <div class="strip strip-success">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Order updated
  </div>
{/if}
{#if form?.action === 'confirm' && form?.success}
  <div class="strip strip-info">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Order confirmed
  </div>
{/if}
{#if form?.action === 'fulfill' && form?.success}
  <div class="strip strip-success">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Order fulfilled
  </div>
{/if}
{#if form?.action === 'cancel' && form?.success}
  <div class="strip strip-warning">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6 9 17l-5-5"/></svg>
    Order cancelled
  </div>
{/if}

<div class="layout">
  <!-- Left: editable fields + read-only meta -->
  <div>
    <!-- Editable fields -->
    <div class="card">
      <div class="card-header">
        <p class="card-title">Order details</p>
        {#if isTerminal}
          <p class="card-sub">This order is {data.order.status} and cannot be edited</p>
        {/if}
      </div>

      <form method="POST" action="?/update" use:enhance>
        <!-- Hidden inputs for frozen/chilled derived from tempMode -->
        <input type="hidden" name="frozen" value={tempMode === 'frozen' ? 'true' : 'false'} />
        <input type="hidden" name="chilled" value={tempMode === 'chilled' ? 'true' : 'false'} />

        <div class="card-body">
          <div class="field-row">
            <div class="field">
              <label for="amount">Amount</label>
              <input
                id="amount"
                name="amount"
                type="number"
                min="1"
                step="1"
                class:error={form?.errors?.amount}
                value={form?.values?.amount ?? data.order.amount}
                disabled={isTerminal}
              />
              {#if form?.errors?.amount}
                <span class="field-error">{form.errors.amount}</span>
              {/if}
            </div>

            <div class="field">
              <label for="meal_category">Meal category</label>
              <select
                id="meal_category"
                name="meal_category"
                class:error={form?.errors?.meal_category}
                disabled={isTerminal}
              >
                {#each MEAL_CATEGORIES as cat}
                  <option
                    value={cat}
                    selected={(form?.values?.meal_category ?? data.order.meal_category) === cat}
                  >
                    {cat}
                  </option>
                {/each}
              </select>
              {#if form?.errors?.meal_category}
                <span class="field-error">{form.errors.meal_category}</span>
              {/if}
            </div>
          </div>

          <div class="field-row">
            <div class="field">
              <label for="serving_date">Serving date</label>
              <input
                id="serving_date"
                name="serving_date"
                type="date"
                class:error={form?.errors?.serving_date}
                value={form?.values?.serving_date ?? toInputDate(data.order.serving_date)}
                disabled={isTerminal}
              />
              {#if form?.errors?.serving_date}
                <span class="field-error">{form.errors.serving_date}</span>
              {/if}
            </div>

            <div class="field">
              <label for="shipment_date">Shipment date</label>
              <input
                id="shipment_date"
                name="shipment_date"
                type="date"
                class:error={form?.errors?.shipment_date}
                value={form?.values?.shipment_date ?? toInputDate(data.order.shipment_date)}
                disabled={isTerminal}
              />
              {#if form?.errors?.shipment_date}
                <span class="field-error">{form.errors.shipment_date}</span>
              {/if}
              <span class="field-hint">Must be on or before serving date</span>
            </div>
          </div>

          <div class="field">
            <span class="field-label">Temperature</span>
            <div class="temp-pills">
              <button
                type="button"
                class="temp-pill"
                class:active={tempMode === 'none'}
                disabled={isTerminal}
                onclick={() => (tempMode = 'none')}
              >
                Ambient
              </button>
              <button
                type="button"
                class="temp-pill"
                class:active={tempMode === 'chilled'}
                disabled={isTerminal}
                onclick={() => (tempMode = 'chilled')}
              >
                🧊 Chilled
              </button>
              <button
                type="button"
                class="temp-pill"
                class:active={tempMode === 'frozen'}
                disabled={isTerminal}
                onclick={() => (tempMode = 'frozen')}
              >
                ❄ Frozen
              </button>
            </div>
            {#if form?.errors?.frozen}
              <span class="field-error">{form.errors.frozen}</span>
            {/if}
          </div>

          <div class="field">
            <label for="notes">Notes <span style="font-weight: 400; color: var(--motm-text-muted); margin-left: 4px;">(optional)</span></label>
            <textarea
              id="notes"
              name="notes"
              rows="2"
              disabled={isTerminal}
            >{form?.values?.notes ?? data.order.client_note ?? ''}</textarea>
          </div>
        </div>

        {#if !isTerminal}
          <div class="card-footer">
            <button type="submit" class="btn btn-primary">Save changes</button>
          </div>
        {/if}
      </form>
    </div>

    <!-- Read-only metadata -->
    <div class="card">
      <div class="card-header">
        <p class="card-title">Order metadata</p>
        <p class="card-sub">These fields are set automatically and cannot be changed</p>
      </div>
      <div class="card-body">
        <div class="meta-grid">
          <div class="ro-field">
            <span class="ro-label">Company</span>
            <span class="ro-value">{data.order.company.name}</span>
          </div>
          <div class="ro-field">
            <span class="ro-label">Recipe</span>
            <span class="ro-value">{data.order.recipe?.name}</span>
          </div>
          <div class="ro-field">
            <span class="ro-label">Source</span>
            <span class="source-chip">{data.order.order_source.replace(/_/g, ' ')}</span>
          </div>
          <div class="ro-field">
            <span class="ro-label">Created</span>
            <span class="ro-value muted">{formatDate(data.order.created_at)}</span>
          </div>
        </div>
      </div>
    </div>
  </div>

  <!-- Right: status transitions panel -->
  <div>
    <div class="transitions-card">
      <div class="transitions-header">
        <p class="card-title">Status</p>
        <p style="margin-top: 6px;">
          <span class="status-badge" style="background: {statusConfig[data.order.status].bg}; color: {statusConfig[data.order.status].color}; font-size: 12px; padding: 2px 8px;">
            {statusConfig[data.order.status].label}
          </span>
        </p>
      </div>

      {#if isTerminal}
        <p class="terminal-notice">
          This order is <strong>{data.order.status}</strong> — no further status changes are possible.
        </p>
      {:else}
        <div class="transitions-body">
          {#if canConfirm}
            <form method="POST" action="?/confirm" use:enhance>
              <button type="submit" class="transition-btn btn-confirm">
                <span>Confirm order</span>
                <span class="arrow">→</span>
              </button>
            </form>
          {/if}

          {#if canFulfill}
            <form method="POST" action="?/fulfill" use:enhance>
              <button type="submit" class="transition-btn btn-fulfill">
                <span>Mark as fulfilled</span>
                <span class="arrow">→</span>
              </button>
            </form>
          {/if}

          {#if canCancel}
            <form method="POST" action="?/cancel" use:enhance>
              <button type="submit" class="transition-btn btn-cancel">
                <span>Cancel order</span>
                <span class="arrow">→</span>
              </button>
            </form>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Lifecycle reminder -->
    <div style="padding: 12px 16px; background: #FAF8F6; border: 1px solid var(--motm-border); border-radius: 8px; font-size: 12px; color: var(--motm-text-muted); line-height: 1.6;">
      <p style="font-weight: 600; color: var(--motm-text); margin-bottom: 6px; font-size: 12.5px;">Status lifecycle</p>
      <p>Pending → Confirmed → Fulfilled</p>
      <p>Pending or Confirmed → Cancelled</p>
      <p style="margin-top: 6px;">Fulfilled and cancelled orders are final.</p>
    </div>
  </div>
</div>
