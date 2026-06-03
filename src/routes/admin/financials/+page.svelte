<script lang="ts">
  import type { PageData, ActionData } from './$types';
  import { enhance } from '$app/forms';
  import Pagination from '$lib/components/Pagination.svelte';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  let invoiceCompany = $state('');
  let invoiceFrom = $state('');
  let invoiceTo = $state('');

  function formatCurrency(val: number | null): string {
    if (val == null) return '—';
    return `$${val.toFixed(2)}`;
  }

  function formatDate(val: string | Date): string {
    return new Date(val).toLocaleDateString('en-GB', {
      day: 'numeric', month: 'short', year: 'numeric',
    });
  }

  function marginColor(pct: number | null): string {
    if (pct == null) return '#999';
    if (pct < 30) return '#DC2626';
    if (pct < 50) return '#D97706';
    return '#16A34A';
  }

  function downloadCsv() {
    if (!form?.invoice) return;
    const invoice = form.invoice;
    const header = 'Date,Recipe,Qty,Unit Price,Line Total';
    const rows = invoice.line_items.map((item: any) =>
      `${new Date(item.serving_date).toISOString().slice(0, 10)},${item.recipe_name},${item.amount},${item.unit_price.toFixed(2)},${item.line_total.toFixed(2)}`
    );
    rows.push(`,,,,`);
    rows.push(`,,,Subtotal,${invoice.subtotal.toFixed(2)}`);

    const csv = [header, ...rows].join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `invoice-${invoice.company_name.toLowerCase().replace(/\s+/g, '-')}-${form.from}-to-${form.to}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<style>
  .page-header {
    margin-bottom: 24px;
  }
  h1 { font-size: 22px; font-weight: 600; color: var(--motm-text); letter-spacing: -0.02em; }
  .subtitle { font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px; }

  .sections {
    display: flex;
    flex-direction: column;
    gap: 32px;
  }

  /* Section card */
  .section-card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    overflow: hidden;
  }
  .section-header {
    padding: 16px 20px;
    border-bottom: 1px solid var(--motm-border);
    background: #FAF8F6;
  }
  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--motm-text);
  }
  .section-subtitle {
    font-size: 12.5px;
    color: var(--motm-text-muted);
    margin-top: 2px;
  }

  /* Margins table */
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
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
    background: #FAF8F6;
  }
  th.right, td.right { text-align: right; }
  td {
    padding: 10px 16px;
    color: var(--motm-text);
    border-bottom: 1px solid var(--motm-border);
    vertical-align: middle;
  }
  tbody tr:last-child td { border-bottom: none; }
  tbody tr:hover td { background: #FAF8F6; }
  .td-muted { color: var(--motm-text-muted); }
  .margin-badge {
    display: inline-flex;
    padding: 2px 7px;
    border-radius: 4px;
    font-size: 11.5px;
    font-weight: 600;
    background: #F3F3F1;
  }

  /* Invoice form */
  .invoice-form {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    padding: 16px 20px;
    flex-wrap: wrap;
  }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field label {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--motm-text);
  }
  .field select, .field input[type="date"] {
    padding: 7px 9px;
    border: 1px solid var(--motm-border);
    border-radius: 7px;
    font-size: 12.5px;
    font-family: inherit;
    color: var(--motm-text);
    background: #fff;
    outline: none;
  }
  .field select:focus, .field input[type="date"]:focus { border-color: var(--motm-pink); }
  .generate-btn {
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    background: var(--motm-pink);
    color: #fff;
    border: none;
    cursor: pointer;
    transition: background 120ms;
  }
  .generate-btn:hover { background: var(--motm-pink-dark); }
  .generate-btn:disabled { opacity: 0.5; cursor: default; }

  /* Invoice preview */
  .invoice-preview {
    padding: 20px;
    border-top: 1px solid var(--motm-border);
  }
  .invoice-header {
    margin-bottom: 16px;
  }
  .invoice-company {
    font-size: 16px;
    font-weight: 600;
    color: var(--motm-text);
  }
  .invoice-address {
    font-size: 12.5px;
    color: var(--motm-text-muted);
    margin-top: 2px;
  }
  .invoice-period {
    font-size: 12.5px;
    color: var(--motm-text-muted);
    margin-top: 4px;
  }
  .invoice-subtotal {
    display: flex;
    justify-content: flex-end;
    padding: 12px 16px;
    font-size: 14px;
    font-weight: 600;
    color: var(--motm-text);
    border-top: 2px solid var(--motm-border);
  }
  .invoice-actions {
    display: flex;
    justify-content: flex-end;
    padding: 12px 0 0;
  }
  .csv-btn {
    padding: 7px 14px;
    border-radius: 7px;
    font-size: 12.5px;
    font-weight: 500;
    font-family: inherit;
    background: #F3F3F1;
    color: var(--motm-text);
    border: 1px solid var(--motm-border);
    cursor: pointer;
    transition: background 120ms;
  }
  .csv-btn:hover { background: #E8E8E6; }

  .empty-state {
    padding: 40px;
    text-align: center;
    color: var(--motm-text-muted);
    font-size: 13px;
  }
  .error-banner {
    padding: 10px 16px;
    background: #FEE2E2;
    color: #991B1B;
    font-size: 13px;
    font-weight: 500;
    border-bottom: 1px solid var(--motm-border);
  }
</style>

<div class="page-header">
  <h1>Financials</h1>
  <p class="subtitle">Recipe cost analysis and invoice generation</p>
</div>

<div class="sections">
  <!-- Section A: Invoice Generator -->
  <div class="section-card">
    <div class="section-header">
      <div class="section-title">Invoice Generator</div>
      <div class="section-subtitle">Generates from confirmed and fulfilled orders only</div>
    </div>

    {#if form?.action === 'invoice' && !form.success}
      <div class="error-banner">{form.error}</div>
    {/if}

    <form method="POST" action="?/invoice" use:enhance class="invoice-form">
      <div class="field">
        <label for="inv-company">Company</label>
        <select id="inv-company" name="company_id" bind:value={invoiceCompany}>
          <option value="">Select company...</option>
          {#each data.companies as company}
            <option value={company.id}>{company.name}</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <label for="inv-from">From</label>
        <input id="inv-from" type="date" name="from" bind:value={invoiceFrom} />
      </div>
      <div class="field">
        <label for="inv-to">To</label>
        <input id="inv-to" type="date" name="to" bind:value={invoiceTo} />
      </div>
      <button type="submit" class="generate-btn" disabled={!invoiceCompany || !invoiceFrom || !invoiceTo}>
        Generate
      </button>
    </form>

    {#if form?.action === 'invoice' && form.success && form.invoice}
      <div class="invoice-preview">
        <div class="invoice-header">
          <div class="invoice-company">{form.invoice.company_name}</div>
          {#if form.invoice.shipping_address}
            <div class="invoice-address">{form.invoice.shipping_address}</div>
          {/if}
          <div class="invoice-period">Period: {formatDate(form.from)} — {formatDate(form.to)}</div>
        </div>

        {#if form.invoice.line_items.length === 0}
          <div class="empty-state">No confirmed or fulfilled orders with pricing found for this period</div>
        {:else}
          <table>
            <thead>
              <tr>
                <th>Date</th>
                <th>Recipe</th>
                <th class="right">Qty</th>
                <th class="right">Unit Price</th>
                <th class="right">Line Total</th>
              </tr>
            </thead>
            <tbody>
              {#each form.invoice.line_items as item}
                <tr>
                  <td class="td-muted">{formatDate(item.serving_date)}</td>
                  <td style="font-weight: 500;">{item.recipe_name}</td>
                  <td class="right td-muted">{item.amount}</td>
                  <td class="right td-muted">{formatCurrency(item.unit_price)}</td>
                  <td class="right">{formatCurrency(item.line_total)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          <div class="invoice-subtotal">
            <span>Subtotal: {formatCurrency(form.invoice.subtotal)}</span>
          </div>
          <div class="invoice-actions">
            <button type="button" class="csv-btn" onclick={downloadCsv}>
              Download CSV
            </button>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Section B: Recipe Margins -->
  <div class="section-card">
    <div class="section-header">
      <div class="section-title">Recipe Margins</div>
      <div class="section-subtitle">Ingredient cost vs. meal price — sorted by worst margin first</div>
    </div>
    {#if data.margins.length === 0}
      <div class="empty-state">No active recipes found</div>
    {:else}
      <table>
        <thead>
          <tr>
            <th>Recipe</th>
            <th class="right">Ingredient Cost</th>
            <th class="right">Price / Meal</th>
            <th class="right">Margin</th>
            <th class="right">Margin %</th>
          </tr>
        </thead>
        <tbody>
          {#each data.margins as recipe}
            <tr>
              <td style="font-weight: 500;">{recipe.name}</td>
              <td class="right td-muted">{formatCurrency(recipe.ingredient_cost)}</td>
              <td class="right td-muted">{formatCurrency(recipe.price_per_meal)}</td>
              <td class="right td-muted">{formatCurrency(recipe.margin)}</td>
              <td class="right">
                {#if recipe.margin_pct != null}
                  <span class="margin-badge" style="color: {marginColor(recipe.margin_pct)};">
                    {recipe.margin_pct.toFixed(1)}%
                  </span>
                {:else}
                  <span class="td-muted">—</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    <Pagination
      page={data.pagination.page}
      totalPages={data.pagination.totalPages}
      total={data.pagination.total}
      perPage={data.pagination.perPage}
    />
  </div>
</div>
