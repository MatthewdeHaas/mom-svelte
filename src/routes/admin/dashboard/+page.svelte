<script lang="ts">
  import type { PageData } from './$types';
  import type { OrderStatus } from '$lib/types';

  const { data }: { data: PageData } = $props();

  const statusConfig: Record<OrderStatus, { label: string; color: string; bg: string }> = {
    pending:   { label: 'Pending',   color: '#92600A', bg: '#FEF3C7' },
    confirmed: { label: 'Confirmed', color: '#1D6A8F', bg: '#DBEAFE' },
    fulfilled: { label: 'Fulfilled', color: '#1F6648', bg: '#D1FAE5' },
    cancelled: { label: 'Cancelled', color: '#6B4A4A', bg: '#F3E8E8' },
  };

  function formatDate(val: string | Date): string {
    return new Date(val).toLocaleDateString('en-GB', {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    });
  }
</script>

<style>
  .page-header {
    margin-bottom: 28px;
  }
  .page-title {
    font-size: 22px;
    font-weight: 600;
    color: var(--motm-text);
    letter-spacing: -0.02em;
  }
  .page-subtitle {
    font-size: 13.5px;
    color: var(--motm-text-muted);
    margin-top: 3px;
  }

  /* ── Stat grid ───────────────────────────────── */
  .stat-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 14px;
    margin-bottom: 32px;
  }
  .stat-card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .stat-card.accent {
    background: var(--motm-pink-light);
    border-color: #EAC4D0;
  }
  .stat-label {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--motm-text-muted);
  }
  .stat-card.accent .stat-label {
    color: var(--motm-pink-dark);
  }
  .stat-value {
    font-size: 30px;
    font-weight: 600;
    letter-spacing: -0.03em;
    color: var(--motm-text);
    line-height: 1;
  }
  .stat-card.accent .stat-value {
    color: var(--motm-pink-dark);
  }
  .stat-sub {
    font-size: 12px;
    color: var(--motm-text-muted);
    margin-top: 2px;
  }

  /* ── Recent orders table ─────────────────────── */
  .section-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 12px;
  }
  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--motm-text);
  }
  .section-link {
    font-size: 12.5px;
    color: var(--motm-pink);
    text-decoration: none;
    font-weight: 500;
  }
  .section-link:hover {
    color: var(--motm-pink-dark);
    text-decoration: underline;
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
  thead tr {
    border-bottom: 1px solid var(--motm-border);
  }
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
  tbody tr:last-child td {
    border-bottom: none;
  }
  tbody tr:hover td {
    background: #FAF8F6;
  }
  .td-muted {
    color: var(--motm-text-muted);
  }
  .td-link {
    color: var(--motm-text);
    text-decoration: none;
    font-weight: 500;
  }
  .td-link:hover {
    color: var(--motm-pink-dark);
  }

  .status-badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: 5px;
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.03em;
    white-space: nowrap;
  }

  .empty-state {
    padding: 40px;
    text-align: center;
    color: var(--motm-text-muted);
    font-size: 13.5px;
  }

  @media (max-width: 600px) {
    .col-recipe, .col-source { display: none; }
  }
</style>

<div class="page-header">
  <h1 class="page-title">Dashboard</h1>
  <p class="page-subtitle">
    {new Date().toLocaleDateString('en-GB', { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' })}
  </p>
</div>

<!-- Stat tiles -->
<div class="stat-grid">
  <div class="stat-card accent">
    <span class="stat-label">Orders today</span>
    <span class="stat-value">{data.stats.ordersToday}</span>
    <span class="stat-sub">by serving date</span>
  </div>
  <div class="stat-card">
    <span class="stat-label">Pending</span>
    <span class="stat-value">{data.stats.pending}</span>
    <span class="stat-sub">awaiting confirmation</span>
  </div>
  <div class="stat-card">
    <span class="stat-label">Confirmed</span>
    <span class="stat-value">{data.stats.confirmed}</span>
    <span class="stat-sub">in production</span>
  </div>
  <div class="stat-card">
    <span class="stat-label">Companies</span>
    <span class="stat-value">{data.stats.companies}</span>
    <span class="stat-sub">active clients</span>
  </div>
</div>

<!-- Recent orders -->
<div class="section-header">
  <span class="section-title">Recent orders</span>
  <a href="/admin/orders" class="section-link">View all →</a>
</div>

<div class="table-wrap">
  {#if data.recentOrders.length === 0}
    <div class="empty-state">No orders yet.</div>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Company</th>
          <th class="col-recipe">Recipe</th>
          <th>Serving date</th>
          <th>Amount</th>
          <th class="col-source">Source</th>
          <th>Status</th>
        </tr>
      </thead>
      <tbody>
        {#each data.recentOrders as order}
          {@const status = statusConfig[order.status as OrderStatus]}
          <tr>
            <td>
              <a href="/admin/orders/{order.id}" class="td-link">
                {order.company_name}
              </a>
            </td>
            <td class="col-recipe td-muted">{order.recipe_name}</td>
            <td class="td-muted">{formatDate(order.serving_date)}</td>
            <td>{order.amount}</td>
            <td class="col-source td-muted" style="text-transform: capitalize;">
              {order.order_source.replace('_', ' ')}
            </td>
            <td>
              <span
                class="status-badge"
                style="background: {status.bg}; color: {status.color};"
              >
                {status.label}
              </span>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
