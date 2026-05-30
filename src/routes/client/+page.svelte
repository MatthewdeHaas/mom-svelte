<script lang="ts">
  let { data } = $props();

  function formatDate(d: Date | string) {
    return new Date(d).toLocaleDateString('en-CA', {
      weekday: 'short', month: 'short', day: 'numeric',
    });
  }
</script>

<style>
  h1 { font-size: 22px; font-weight: 600; color: #2C1F24; letter-spacing: -0.02em; margin: 0 0 4px; }
  .sub { font-size: 14px; color: #7A6670; margin: 0 0 32px; }

  .cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 16px; margin-bottom: 32px; }
  .card {
    background: #fff;
    border: 1px solid #EDE8E3;
    border-radius: 12px;
    padding: 20px 24px;
  }
  .card-label { font-size: 11.5px; font-weight: 600; letter-spacing: 0.07em; text-transform: uppercase; color: #B8AEB3; margin-bottom: 8px; }
  .card-value { font-size: 28px; font-weight: 600; color: #2C1F24; letter-spacing: -0.03em; }

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: #2C1F24;
    margin: 0 0 12px;
  }

  .order-list { display: flex; flex-direction: column; gap: 8px; }
  .order-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: #fff;
    border: 1px solid #EDE8E3;
    border-radius: 9px;
    padding: 12px 16px;
    gap: 12px;
  }
  .order-recipe { font-size: 14px; font-weight: 500; color: #2C1F24; }
  .order-date { font-size: 13px; color: #7A6670; }
  .order-qty { font-size: 13px; color: #7A6670; }

  .empty { font-size: 14px; color: #B8AEB3; padding: 20px 0; }
</style>

<h1>Dashboard</h1>
<p class="sub">Here's a summary of your upcoming orders.</p>

<div class="cards">
  <div class="card">
    <div class="card-label">Upcoming orders</div>
    <div class="card-value">{data.upcoming.length}</div>
  </div>
  <div class="card">
    <div class="card-label">Past orders</div>
    <div class="card-value">{data.recentCount}</div>
  </div>
</div>

<p class="section-title">Next 5 upcoming orders</p>
<div class="order-list">
  {#each data.upcoming as order}
    <div class="order-row">
      <div>
        <div class="order-recipe">{order.recipe_name ?? 'Custom order'}</div>
        <div class="order-date">{formatDate(order.serving_date)}</div>
      </div>
      <div class="order-qty">{order.amount} meals</div>
    </div>
  {:else}
    <p class="empty">No upcoming orders.</p>
  {/each}
</div>
