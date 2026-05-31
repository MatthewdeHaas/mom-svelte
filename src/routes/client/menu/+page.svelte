<script lang="ts">
  import type { PageData } from './$types';

  const { data }: { data: PageData } = $props();

  // Group by date for easier rendering
  const byDate = $derived(data.calendarDays);
  const hasDays = $derived(byDate.length > 0);

  const mealColor: Record<string, { bg: string; color: string }> = {
    Breakfast: { bg: '#FEF3C7', color: '#92400E' },
    Lunch:     { bg: '#DBEAFE', color: '#1D4ED8' },
    Snack:     { bg: '#D1FAE5', color: '#065F46' },
    Soup:      { bg: '#EDE9FE', color: '#5B21B6' },
    Dinner:    { bg: '#FCE7F3', color: '#9D174D' },
  };

  function mealStyle(cat: string) {
    const c = mealColor[cat] ?? { bg: '#F3F4F6', color: '#374151' };
    return `background:${c.bg};color:${c.color};`;
  }
</script>

<style>
  h1 { font-size: 22px; font-weight: 600; color: #2C1F24; letter-spacing: -0.02em; margin: 0 0 4px; }
  .sub { font-size: 14px; color: #7A6670; margin: 0 0 28px; }

  .day-group { margin-bottom: 20px; }
  .day-label {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: #7A6670;
    margin: 0 0 8px;
  }

  .slot-grid { display: flex; flex-direction: column; gap: 8px; }
  .slot-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: #fff;
    border: 1px solid #EDE8E3;
    border-radius: 10px;
    padding: 14px 18px;
    gap: 16px;
    flex-wrap: wrap;
  }
  .slot-card.ordered {
    opacity: 0.5;
    pointer-events: none;
  }
  .slot-info { flex: 1; min-width: 0; }
  .recipe-name { font-size: 14px; font-weight: 500; color: #2C1F24; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meal-badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 20px;
    font-size: 11.5px;
    font-weight: 600;
    margin-top: 4px;
  }
  .ordered-tag {
    font-size: 12px;
    font-weight: 600;
    color: #1F6648;
    background: #D1FAE5;
    padding: 3px 9px;
    border-radius: 20px;
    white-space: nowrap;
  }

  .btn {
    padding: 7px 14px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    background: #C9718A;
    color: #fff;
    text-decoration: none;
    white-space: nowrap;
    transition: background 120ms;
  }
  .btn:hover { background: #b05e77; }

  .empty { font-size: 14px; color: #B8AEB3; padding: 40px 0; text-align: center; }
  .empty-title { font-size: 16px; font-weight: 500; color: #7A6670; margin-bottom: 6px; }
</style>

<h1>Menu</h1>
<p class="sub">Upcoming meals available for your account. Click a slot to place an order.</p>

{#if !hasDays}
  <div class="empty">
    <p class="empty-title">No upcoming menu slots</p>
    <p>There are no active menus covering the next 4 weeks. Check back soon or contact your account manager.</p>
  </div>
{:else}
  {#each byDate as day}
    <div class="day-group">
      <p class="day-label">{day.dayLabel}</p>
      <div class="slot-grid">
        {#each day.slots as slot}
          <div class="slot-card" class:ordered={slot.alreadyOrdered}>
            <div class="slot-info">
              <p class="recipe-name">{slot.recipeName}</p>
              <span class="meal-badge" style={mealStyle(slot.mealCategory)}>
                {slot.mealCategory}
              </span>
            </div>
            {#if slot.alreadyOrdered}
              <span class="ordered-tag">✓ Ordered</span>
            {:else}
              <a
                href="/client/orders/new?menu_recipe_id={slot.menuRecipeId}&serving_date={day.date}&recipe_name={encodeURIComponent(slot.recipeName)}&meal_category={encodeURIComponent(slot.mealCategory)}"
                class="btn"
              >
                Order
              </a>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/each}
{/if}
