<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData, ActionData } from './$types';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  // Initialize grid with ALL person×date keys pre-set
  function buildGrid(attendance: Record<string, boolean>): Record<string, boolean> {
    const g: Record<string, boolean> = {};
    for (const person of data.persons) {
      for (const date of data.dates) {
        const key = `${person.id}|${date}`;
        g[key] = attendance[key] ?? false;
      }
    }
    return g;
  }

  let grid = $state(buildGrid(data.attendance));

  function shortDate(d: string): string {
    return new Date(d).toLocaleDateString('en-CA', { month: 'short', day: 'numeric' });
  }

  function dayName(d: string): string {
    return new Date(d).toLocaleDateString('en-CA', { weekday: 'short' });
  }

  // Count attendees for a given date column
  function columnCount(date: string): number {
    return data.persons.filter((p) => grid[`${p.id}|${date}`]).length;
  }

  const hasDates = $derived(data.dates.length > 0);
  const hasPersons = $derived(data.persons.length > 0);
</script>

<style>
  h1 { font-size: 22px; font-weight: 600; color: #2C1F24; letter-spacing: -0.02em; margin: 0 0 4px; }
  .sub { font-size: 14px; color: #7A6670; margin: 0 0 28px; }

  .success-strip {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 14px; background: #D1FAE5; color: #1F6648;
    border-radius: 7px; font-size: 13px; font-weight: 500; margin-bottom: 20px;
  }

  /* Scrollable wrapper so wide grids don't break layout */
  .grid-wrap {
    overflow-x: auto;
    background: #fff;
    border: 1px solid #EDE8E3;
    border-radius: 10px;
    margin-bottom: 20px;
  }

  table { border-collapse: collapse; width: 100%; min-width: max-content; }

  /* Header row */
  thead th {
    padding: 10px 14px;
    font-size: 11.5px;
    font-weight: 600;
    text-align: center;
    color: #7A6670;
    border-bottom: 1px solid #EDE8E3;
    background: #FAF8F6;
    white-space: nowrap;
  }
  thead th.name-col { text-align: left; min-width: 160px; }
  .date-day { font-size: 11px; color: #B8AEB3; font-weight: 400; display: block; }

  /* Body rows */
  tbody tr { border-bottom: 1px solid #EDE8E3; }
  tbody tr:last-child { border-bottom: none; }
  tbody tr:hover td { background: #FBF9F7; }

  td {
    padding: 10px 14px;
    font-size: 13.5px;
    color: #2C1F24;
  }
  td.name-col { font-weight: 500; }
  td.check-col { text-align: center; }

  /* Checkbox cell — large tap target */
  .check-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .check-wrap input[type="checkbox"] {
    width: 18px;
    height: 18px;
    accent-color: #C9718A;
    cursor: pointer;
  }

  /* Count footer row */
  tfoot td {
    padding: 8px 14px;
    font-size: 12px;
    font-weight: 600;
    color: #7A6670;
    text-align: center;
    border-top: 1px solid #EDE8E3;
    background: #FAF8F6;
  }
  tfoot td.name-col { text-align: left; }

  .btn {
    padding: 9px 20px; border-radius: 8px; font-size: 13.5px; font-weight: 500;
    font-family: inherit; cursor: pointer; border: none; transition: background 120ms;
  }
  .btn-primary { background: #C9718A; color: #fff; }
  .btn-primary:hover { background: #b05e77; }

  .empty-card {
    background: #fff; border: 1px solid #EDE8E3; border-radius: 10px;
    padding: 48px 32px; text-align: center;
  }
  .empty-title { font-size: 16px; font-weight: 500; color: #7A6670; margin-bottom: 6px; }
  .empty-body { font-size: 14px; color: #B8AEB3; }
  .empty-body a { color: #C9718A; text-decoration: none; }
  .empty-body a:hover { text-decoration: underline; }
</style>

<h1>Attendance</h1>
<p class="sub">Mark who will be present for each upcoming service date.</p>

{#if form?.action === 'save' && form?.success}
  <div class="success-strip">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M20 6 9 17l-5-5"/>
    </svg>
    Attendance saved
  </div>
{/if}

{#if !hasPersons}
  <div class="empty-card">
    <p class="empty-title">No people on your roster</p>
    <p class="empty-body">
      <a href="/client/roster">Add people to your roster</a> before tracking attendance.
    </p>
  </div>
{:else if !hasDates}
  <div class="empty-card">
    <p class="empty-title">No upcoming service dates</p>
    <p class="empty-body">
      Attendance is shown for dates with confirmed or pending orders.
      <a href="/client/menu">Place an order</a> to see dates here.
    </p>
  </div>
{:else}
  <form method="POST" action="?/save" use:enhance={({ formData, formElement }) => {
      // Read checked state directly from DOM checkboxes — bind:checked keeps
      // them in sync with component state, but we read DOM to avoid any
      // stale closure issues with use:enhance action callbacks.
      const checkboxes = formElement.querySelectorAll<HTMLInputElement>('input[type="checkbox"][data-att-key]');
      for (const cb of checkboxes) {
        const key = cb.dataset.attKey!;
        const [personId, date] = key.split('|');
        formData.set(`att|${personId}|${date}`, cb.checked ? '1' : '0');
      }
      return async ({ update }) => {
        await update({ reset: false });
        // Re-sync grid from fresh server data after save + invalidateAll
        grid = buildGrid(data.attendance);
      };
    }}>
    <!--
      Hidden inputs are kept for progressive enhancement (no-JS fallback)
      but the enhance callback above overrides FormData with live grid state.
    -->
    {#each data.persons as person}
      {#each data.dates as date}
        <input
          type="hidden"
          name="att|{person.id}|{date}"
          value={grid[`${person.id}|${date}`] ? '1' : '0'}
        />
      {/each}
    {/each}

    <div class="grid-wrap">
      <table>
        <thead>
          <tr>
            <th class="name-col">Person</th>
            {#each data.dates as date}
              <th>
                {shortDate(date)}
                <span class="date-day">{dayName(date)}</span>
              </th>
            {/each}
          </tr>
        </thead>

        <tbody>
          {#each data.persons as person}
            <tr>
              <td class="name-col">{person.name}</td>
              {#each data.dates as date}
                <td class="check-col">
                  <div class="check-wrap">
                    <input
                      type="checkbox"
                      data-att-key={`${person.id}|${date}`}
                      bind:checked={grid[`${person.id}|${date}`]}
                    />
                  </div>
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>

        <tfoot>
          <tr>
            <td class="name-col">Total attending</td>
            {#each data.dates as date}
              <td>{columnCount(date)}</td>
            {/each}
          </tr>
        </tfoot>
      </table>
    </div>

    <button type="submit" class="btn btn-primary">Save attendance</button>
  </form>
{/if}
