<script lang="ts">
  import type { OrderSummary } from '$lib/types';

  let { data } = $props();

  function formatDate(d: Date | string) {
    return new Date(d).toLocaleDateString('en-CA', {
      weekday: 'short', month: 'short', day: 'numeric',
    });
  }

  const statusLabel: Record<string, string> = {
    pending:   'Pending',
    confirmed: 'Confirmed',
    fulfilled: 'Fulfilled',
    cancelled: 'Cancelled',
  };

  const statusColor: Record<string, string> = {
    pending:   '#F59E0B',
    confirmed: '#10B981',
    fulfilled: '#6B7280',
    cancelled: '#EF4444',
  };
</script>

<style>
  h1 { font-size: 22px; font-weight: 600; color: #2C1F24; letter-spacing: -0.02em; margin: 0 0 4px; }
  .sub { font-size: 14px; color: #7A6670; margin: 0 0 24px; }

  table { width: 100%; border-collapse: collapse; background: #fff; border: 1px solid #EDE8E3; border-radius: 12px; overflow: hidden; }
  th { font-size: 11.5px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase; color: #B8AEB3; text-align: left; padding: 12px 16px; border-bottom: 1px solid #EDE8E3; background: #FAF8F6; }
  td { font-size: 13.5px; color: #2C1F24; padding: 12px 16px; border-bottom: 1px solid #EDE8E3; }
  tr:last-child td { border-bottom: none; }
  tr:hover td { background: #FBF9F7; }

  .badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 20px;
    font-size: 11.5px;
    font-weight: 600;
    background: color-mix(in srgb, var(--c) 12%, transparent);
    color: var(--c);
  }

  .empty { font-size: 14px; color: #B8AEB3; padding: 32px 0; text-align: center; }
</style>

<h1>Orders</h1>
<p class="sub">All orders placed for your account.</p>

{#if data.orders.length === 0}
  <p class="empty">No orders yet.</p>
{:else}
  <table>
    <thead>
      <tr>
        <th>Meal</th>
        <th>Serving date</th>
        <th>Amount</th>
        <th>Status</th>
      </tr>
    </thead>
    <tbody>
      {#each data.orders as order}
        <tr>
          <td>{order.recipe_name ?? 'Custom order'}</td>
          <td>{formatDate(order.serving_date)}</td>
          <td>{order.amount}</td>
          <td>
            <span class="badge" style="--c: {statusColor[order.status] ?? '#6B7280'}">
              {statusLabel[order.status] ?? order.status}
            </span>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
