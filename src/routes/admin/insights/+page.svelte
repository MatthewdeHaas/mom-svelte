<script lang="ts">
  import type { PageData } from './$types';
  import type { OrderSource } from '$lib/types';
  import { todayEST, shiftDate, formatDateDisplay } from '$lib/utils/dates';
  import InsightPanel from '$lib/components/insights/InsightPanel.svelte';
  import LineChart from '$lib/components/insights/LineChart.svelte';
  import BarChart from '$lib/components/insights/BarChart.svelte';
  import DoughnutChart from '$lib/components/insights/DoughnutChart.svelte';

  const { data }: { data: PageData } = $props();

  // Date range state (default: last 30 days)
  let rangeTo = $state(todayEST());
  let rangeFrom = $state(shiftDate(todayEST(), -30));

  // Panel states
  type PanelState<T> = { loading: boolean; error: string | null; data: T | null };

  let volume = $state<PanelState<{ date: string; total: number }[]>>({ loading: true, error: null, data: null });
  let topRecipes = $state<PanelState<{ name: string; order_count: number; total_amount: number; percent_of_total: number }[]>>({ loading: true, error: null, data: null });
  let companies = $state<PanelState<{ name: string; total_amount: number; order_count: number }[]>>({ loading: true, error: null, data: null });
  let sourceMix = $state<PanelState<{ order_source: OrderSource; total_amount: number; order_count: number }[]>>({ loading: true, error: null, data: null });
  let ingredients = $state<PanelState<{ name: string; total_amount: number; unit: string; allergens: string }[]>>({ loading: true, error: null, data: null });
  let attendance = $state<PanelState<{ trend: { date: string; attending: number; total: number; rate: number }[]; byCompany: { company_name: string; attending: number; total: number; rate: number }[] }>>({ loading: true, error: null, data: null });

  let granularity = $state<'daily' | 'weekly'>('daily');

  const sourceLabels: Record<OrderSource, string> = {
    admin_manual: 'Admin',
    excel_import: 'Excel Import',
    client_portal: 'Client Portal',
  };

  async function fetchPanel<T>(endpoint: string, params: Record<string, string> = {}): Promise<T> {
    const qs = new URLSearchParams({ from: rangeFrom, to: rangeTo, ...params });
    const res = await fetch(`/admin/insights/api/${endpoint}?${qs}`);
    if (!res.ok) throw new Error(await res.text());
    return res.json();
  }

  async function loadAllPanels() {
    volume = { loading: true, error: null, data: null };
    topRecipes = { loading: true, error: null, data: null };
    companies = { loading: true, error: null, data: null };
    sourceMix = { loading: true, error: null, data: null };
    ingredients = { loading: true, error: null, data: null };
    attendance = { loading: true, error: null, data: null };

    fetchPanel<{ date: string; total: number }[]>('volume', { granularity })
      .then((d) => { volume = { loading: false, error: null, data: d }; })
      .catch((e) => { volume = { loading: false, error: e.message, data: null }; });

    fetchPanel<typeof topRecipes.data>('top-recipes')
      .then((d) => { topRecipes = { loading: false, error: null, data: d }; })
      .catch((e) => { topRecipes = { loading: false, error: e.message, data: null }; });

    fetchPanel<typeof companies.data>('companies')
      .then((d) => { companies = { loading: false, error: null, data: d }; })
      .catch((e) => { companies = { loading: false, error: e.message, data: null }; });

    fetchPanel<typeof sourceMix.data>('source-mix')
      .then((d) => { sourceMix = { loading: false, error: null, data: d }; })
      .catch((e) => { sourceMix = { loading: false, error: e.message, data: null }; });

    fetchPanel<typeof ingredients.data>('ingredients')
      .then((d) => { ingredients = { loading: false, error: null, data: d }; })
      .catch((e) => { ingredients = { loading: false, error: e.message, data: null }; });

    fetchPanel<{ trend: typeof attendance.data extends { trend: infer T } | null ? T : never; byCompany: typeof attendance.data extends { byCompany: infer T } | null ? T : never }>('attendance')
      .then((d) => { attendance = { loading: false, error: null, data: d as any }; })
      .catch((e) => { attendance = { loading: false, error: e.message, data: null }; });
  }

  // Load panels on mount and when range changes
  $effect(() => {
    void rangeFrom;
    void rangeTo;
    void granularity;
    loadAllPanels();
  });

  // Compute overall attendance rate from trend data
  let overallAttendance = $derived.by(() => {
    if (!attendance.data?.trend?.length) return null;
    const totAttending = attendance.data.trend.reduce((s, d) => s + d.attending, 0);
    const totAll = attendance.data.trend.reduce((s, d) => s + d.total, 0);
    if (totAll === 0) return null;
    return Math.round((totAttending / totAll) * 1000) / 10;
  });
</script>

<style>
  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 28px;
    flex-wrap: wrap;
    gap: 12px;
  }
  .page-title {
    font-size: 22px;
    font-weight: 600;
    color: var(--motm-text);
    letter-spacing: -0.02em;
  }

  .date-controls {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .date-controls label {
    font-size: 12px;
    font-weight: 500;
    color: var(--motm-text-muted);
  }
  .date-controls input[type="date"] {
    border: 1px solid var(--motm-border);
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 13px;
    color: var(--motm-text);
    background: var(--motm-surface);
  }
  .granularity-toggle {
    display: flex;
    border: 1px solid var(--motm-border);
    border-radius: 6px;
    overflow: hidden;
  }
  .granularity-toggle button {
    border: none;
    background: var(--motm-surface);
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 500;
    color: var(--motm-text-muted);
    cursor: pointer;
  }
  .granularity-toggle button.active {
    background: var(--motm-pink);
    color: white;
  }

  /* ── Stat grid ───────────────────────────────── */
  .stat-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 14px;
    margin-bottom: 28px;
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
  .stat-sub.positive { color: #1F6648; }
  .stat-sub.negative { color: #9E4E66; }

  /* ── Panel grid ─────────────────────────────── */
  .panel-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
    margin-bottom: 18px;
  }
  .panel-full {
    grid-column: 1 / -1;
  }

  @media (max-width: 768px) {
    .panel-grid {
      grid-template-columns: 1fr;
    }
  }

  /* ── Table within panels ────────────────────── */
  .insight-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  .insight-table th {
    text-align: left;
    padding: 8px 12px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--motm-text-muted);
    border-bottom: 1px solid var(--motm-border);
  }
  .insight-table td {
    padding: 8px 12px;
    color: var(--motm-text);
    border-bottom: 1px solid var(--motm-border);
  }
  .insight-table tbody tr:last-child td {
    border-bottom: none;
  }
  .insight-table .td-muted {
    color: var(--motm-text-muted);
  }
  .insight-table .td-right {
    text-align: right;
  }

  .empty-panel {
    text-align: center;
    color: var(--motm-text-muted);
    font-size: 13px;
    padding: 24px;
  }
</style>

<div class="page-header">
  <h1 class="page-title">Insights</h1>
  <div class="date-controls">
    <label for="insights-from">From</label>
    <input id="insights-from" type="date" bind:value={rangeFrom} />
    <label for="insights-to">To</label>
    <input id="insights-to" type="date" bind:value={rangeTo} />
    <div class="granularity-toggle">
      <button class:active={granularity === 'daily'} onclick={() => granularity = 'daily'}>Daily</button>
      <button class:active={granularity === 'weekly'} onclick={() => granularity = 'weekly'}>Weekly</button>
    </div>
  </div>
</div>

<!-- KPI Cards (server-rendered, instant) -->
<div class="stat-grid">
  <div class="stat-card accent">
    <span class="stat-label">Meals this week</span>
    <span class="stat-value">{Math.round(data.kpi.totalMeals)}</span>
    {#if data.kpi.weekOverWeekChange !== null}
      <span class="stat-sub" class:positive={data.kpi.weekOverWeekChange >= 0} class:negative={data.kpi.weekOverWeekChange < 0}>
        {data.kpi.weekOverWeekChange >= 0 ? '+' : ''}{data.kpi.weekOverWeekChange}% vs last week
      </span>
    {:else}
      <span class="stat-sub">no prior data</span>
    {/if}
  </div>
  <div class="stat-card">
    <span class="stat-label">Pending</span>
    <span class="stat-value">{data.kpi.statusCounts.pending}</span>
    <span class="stat-sub">awaiting confirmation</span>
  </div>
  <div class="stat-card">
    <span class="stat-label">Confirmed</span>
    <span class="stat-value">{data.kpi.statusCounts.confirmed}</span>
    <span class="stat-sub">in production</span>
  </div>
  <div class="stat-card">
    <span class="stat-label">Active companies</span>
    <span class="stat-value">{data.kpi.activeCompanies}</span>
    <span class="stat-sub">with orders this week</span>
  </div>
</div>

<!-- Charts row 1: Volume + Source Mix -->
<div class="panel-grid">
  <InsightPanel title="Order Volume" loading={volume.loading} error={volume.error}>
    {#if volume.data && volume.data.length > 0}
      <LineChart
        labels={volume.data.map(d => d.date.slice(5))}
        data={volume.data.map(d => d.total)}
        label="Meals"
      />
    {:else}
      <p class="empty-panel">No order data for this period.</p>
    {/if}
  </InsightPanel>

  <InsightPanel title="Order Source" loading={sourceMix.loading} error={sourceMix.error}>
    {#if sourceMix.data && sourceMix.data.length > 0}
      <DoughnutChart
        labels={sourceMix.data.map(d => sourceLabels[d.order_source] ?? d.order_source)}
        data={sourceMix.data.map(d => d.total_amount)}
      />
    {:else}
      <p class="empty-panel">No order data for this period.</p>
    {/if}
  </InsightPanel>

  <!-- Row 2: Top Recipes + Company Breakdown -->
  <InsightPanel title="Top Recipes" loading={topRecipes.loading} error={topRecipes.error}>
    {#if topRecipes.data && topRecipes.data.length > 0}
      <table class="insight-table">
        <thead>
          <tr>
            <th>#</th>
            <th>Recipe</th>
            <th class="td-right">Meals</th>
            <th class="td-right">%</th>
          </tr>
        </thead>
        <tbody>
          {#each topRecipes.data as recipe, i}
            <tr>
              <td class="td-muted">{i + 1}</td>
              <td>{recipe.name}</td>
              <td class="td-right">{Math.round(recipe.total_amount)}</td>
              <td class="td-right td-muted">{recipe.percent_of_total}%</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="empty-panel">No recipe data for this period.</p>
    {/if}
  </InsightPanel>

  <InsightPanel title="Company Breakdown" loading={companies.loading} error={companies.error}>
    {#if companies.data && companies.data.length > 0}
      <BarChart
        labels={companies.data.map(d => d.name)}
        data={companies.data.map(d => d.total_amount)}
        label="Meals"
        horizontal={true}
      />
    {:else}
      <p class="empty-panel">No company data for this period.</p>
    {/if}
  </InsightPanel>

  <!-- Row 3: Ingredient Demand (full width) -->
  <div class="panel-full">
    <InsightPanel title="Ingredient Demand (Pending + Confirmed)" loading={ingredients.loading} error={ingredients.error}>
      {#if ingredients.data && ingredients.data.length > 0}
        <table class="insight-table">
          <thead>
            <tr>
              <th>Ingredient</th>
              <th class="td-right">Amount</th>
              <th>Unit</th>
              <th>Allergens</th>
            </tr>
          </thead>
          <tbody>
            {#each ingredients.data as ing}
              <tr>
                <td>{ing.name}</td>
                <td class="td-right">{Math.round(ing.total_amount * 10) / 10}</td>
                <td class="td-muted">{ing.unit}</td>
                <td class="td-muted">{ing.allergens || '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="empty-panel">No pending/confirmed orders with recipes in this period.</p>
      {/if}
    </InsightPanel>
  </div>

  <!-- Row 4: Attendance -->
  <InsightPanel title="Attendance Trend" loading={attendance.loading} error={attendance.error}>
    {#if attendance.data?.trend && attendance.data.trend.length > 0}
      {#if overallAttendance !== null}
        <p style="font-size: 13px; color: var(--motm-text-muted); margin-bottom: 12px;">
          Overall: <strong style="color: var(--motm-text); font-size: 18px;">{overallAttendance}%</strong>
        </p>
      {/if}
      <LineChart
        labels={attendance.data.trend.map(d => d.date.slice(5))}
        data={attendance.data.trend.map(d => d.rate)}
        label="Attendance %"
        color="#1D6A8F"
      />
    {:else}
      <p class="empty-panel">No attendance data for this period.</p>
    {/if}
  </InsightPanel>

  <InsightPanel title="Attendance by Company" loading={attendance.loading} error={attendance.error}>
    {#if attendance.data?.byCompany && attendance.data.byCompany.length > 0}
      <table class="insight-table">
        <thead>
          <tr>
            <th>Company</th>
            <th class="td-right">Attending</th>
            <th class="td-right">Total</th>
            <th class="td-right">Rate</th>
          </tr>
        </thead>
        <tbody>
          {#each attendance.data.byCompany as row}
            <tr>
              <td>{row.company_name}</td>
              <td class="td-right">{row.attending}</td>
              <td class="td-right td-muted">{row.total}</td>
              <td class="td-right"><strong>{row.rate}%</strong></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="empty-panel">No attendance data for this period.</p>
    {/if}
  </InsightPanel>
</div>
