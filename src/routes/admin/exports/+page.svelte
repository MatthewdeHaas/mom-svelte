<script lang="ts">
  import { fade } from 'svelte/transition';
  import { Calendar } from '$lib/components/ui/calendar/index.js';
  import * as Popover from '$lib/components/ui/popover/index.js';
  import { parseDate } from '@internationalized/date';
  import type { CalendarDate, DateValue } from '@internationalized/date';
  import {
    todayEST,
    weekBoundsEST,
    formatDateDisplay,
    formatWeekDisplay,
    shiftDate,
  } from '$lib/utils/dates';

  const EXPORTS = [
    {
      id: 'productionOrder',
      endpoint: 'production-order',
      title: 'Production order',
      description: 'Per-recipe production quantities broken down by meal category. Used by the kitchen team.',
      format: 'Excel (.xlsx)',
      icon: 'utensils',
      dateLabel: 'Shipment date',
    },
    {
      id: 'shoppingList',
      endpoint: 'shopping-list',
      title: 'Shopping list',
      description: 'Consolidated ingredient quantities across all orders. Used for purchasing.',
      format: 'Excel (.xlsx)',
      icon: 'cart',
      dateLabel: 'Shipment date',
    },
    {
      id: 'packingSlips',
      endpoint: 'packing-slips',
      title: 'Packing slips',
      description: 'Per-company packing slips for the selected week, ready for printing and dispatch.',
      format: 'PDF',
      icon: 'document',
      dateLabel: 'Any date in the week',
    },
  ] as const;

  type Mode = 'week' | 'day';

  type PreviewData =
    | { recipes: number; totalMeals: number }
    | { ingredients: number; orders: number }
    | { slips: number; companies: number };

  type CardState =
    | { phase: 'idle' }
    | { phase: 'loading' }
    | { phase: 'preview'; data: PreviewData }
    | { phase: 'error'; message: string };

  let modes        = $state<Mode[]>(EXPORTS.map(() => 'week'));
  let dates        = $state<string[]>(EXPORTS.map(() => todayEST()));
  let states       = $state<CardState[]>(EXPORTS.map(() => ({ phase: 'idle' })));
  let popoverOpen  = $state<boolean[]>(EXPORTS.map(() => false));

  function toCalendarDate(dateStr: string): CalendarDate {
    return parseDate(dateStr);
  }

  function onCalendarSelect(i: number, val: DateValue | undefined) {
    if (!val) return;
    dates[i]       = val.toString();
    states[i]      = { phase: 'idle' };
    popoverOpen[i] = false;
  }

  function weekOf(i: number) {
    return weekBoundsEST(dates[i]);
  }

  function buildUrl(i: number, path: string): string {
    const base = `/admin/exports/${path}`;
    if (modes[i] === 'week') return `${base}?week_of=${dates[i]}`;
    return `${base}?shipment_date=${dates[i]}`;
  }

  function jumpToday(i: number) {
    modes[i]  = 'day';
    dates[i]  = todayEST();
    states[i] = { phase: 'idle' };
  }

  function jumpThisWeek(i: number) {
    modes[i]  = 'week';
    dates[i]  = todayEST();
    states[i] = { phase: 'idle' };
  }

  function navigate(i: number, delta: number) {
    const shift = modes[i] === 'week' ? delta * 7 : delta;
    dates[i]  = shiftDate(dates[i], shift);
    states[i] = { phase: 'idle' };
  }

  function setMode(i: number, mode: Mode) {
    modes[i]  = mode;
    states[i] = { phase: 'idle' };
  }

  function pickerLabel(i: number): string {
    const todayStr = todayEST();
    if (modes[i] === 'week') {
      const { from, to } = weekOf(i);
      const { from: thisFrom } = weekBoundsEST(todayStr);
      const suffix = from === thisFrom ? ' · this week' : '';
      return formatWeekDisplay(from, to) + suffix;
    }
    const suffix = dates[i] === todayStr ? ' · today' : '';
    return formatDateDisplay(dates[i]) + suffix;
  }

  async function fetchPreview(i: number, endpoint: string) {
    states[i] = { phase: 'loading' };
    try {
      const res = await fetch(buildUrl(i, `${endpoint}/preview`));
      if (!res.ok) throw new Error(await res.text());
      states[i] = { phase: 'preview', data: await res.json() };
    } catch {
      states[i] = { phase: 'error', message: 'Could not load preview. Try again.' };
    }
  }

  function reset(i: number) {
    states[i] = { phase: 'idle' };
  }

  function previewLines(endpoint: string, data: PreviewData): string[] {
    if (endpoint === 'production-order' && 'recipes' in data)
      return [`${data.recipes} recipes`, `${data.totalMeals} total meals`];
    if (endpoint === 'shopping-list' && 'ingredients' in data)
      return [`${data.ingredients} ingredients`, `across ${data.orders} orders`];
    if (endpoint === 'packing-slips' && 'slips' in data)
      return [`${data.slips} packing slips`, `for ${data.companies} companies`];
    return [];
  }
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

  .export-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
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
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .mode-tabs {
    display: flex;
    gap: 4px;
    background: #EFEFED;
    border-radius: 7px;
    padding: 3px;
  }
  .mode-tab {
    flex: 1;
    padding: 5px 0;
    border: none;
    border-radius: 5px;
    font-size: 12.5px;
    font-weight: 500;
    font-family: inherit;
    background: transparent;
    color: var(--motm-text-muted);
    cursor: pointer;
    transition: background 100ms, color 100ms;
  }
  .mode-tab.active {
    background: #fff;
    color: var(--motm-text);
    box-shadow: 0 1px 3px rgba(0,0,0,0.08);
  }

  .shortcuts {
    display: flex;
    gap: 6px;
  }
  .shortcut-btn {
    flex: 1;
    padding: 5px 8px;
    border: 1.5px solid var(--motm-border);
    border-radius: 6px;
    font-size: 11.5px;
    font-weight: 600;
    font-family: inherit;
    background: #fff;
    color: var(--motm-text-muted);
    cursor: pointer;
    transition: border-color 120ms, color 120ms;
    text-align: center;
  }
  .shortcut-btn:hover { border-color: var(--motm-pink); color: var(--motm-text); }
  .shortcut-btn.active {
    border-color: var(--motm-pink);
    color: var(--motm-pink-dark);
    background: var(--motm-pink-light);
  }

  .picker {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .picker-nav {
    width: 28px;
    height: 28px;
    border: 1px solid var(--motm-border);
    border-radius: 6px;
    background: #fff;
    color: var(--motm-text-muted);
    font-size: 14px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: border-color 120ms;
  }
  .picker-nav:hover { border-color: var(--motm-pink); color: var(--motm-text); }

  /* The label is now a Popover.Trigger styled to match the old span */
  :global(.picker-label-btn) {
    flex: 1;
    text-align: center;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--motm-text);
    line-height: 1.35;
    padding: 4px 6px;
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 6px;
    min-height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    cursor: pointer;
    font-family: inherit;
    transition: border-color 120ms;
    width: 100%;
  }
  :global(.picker-label-btn:hover) { border-color: var(--motm-pink); }
  :global(.picker-label-btn svg)   { flex-shrink: 0; color: var(--motm-text-muted); }

  /* Tint the selected day and today indicator to motm pink */
  :global(.motm-cal [data-selected] > *) {
    background-color: var(--motm-pink) !important;
    color: #fff !important;
  }
  :global(.motm-cal [data-today]:not([data-selected]) > *) {
    background-color: var(--motm-pink-light) !important;
    color: var(--motm-pink-dark) !important;
  }

  .preview-box {
    background: #F0F7FF;
    border: 1px solid #C7DEFF;
    border-radius: 8px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .preview-label {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #3B6FC4;
    margin-bottom: 4px;
  }
  .preview-line {
    font-size: 13px;
    color: var(--motm-text);
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .preview-line::before {
    content: '';
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #3B6FC4;
    flex-shrink: 0;
  }
  .preview-empty {
    font-size: 13px;
    color: var(--motm-text-muted);
    font-style: italic;
  }

  .error-box {
    background: #FEF2F2;
    border: 1px solid #FECACA;
    border-radius: 8px;
    padding: 10px 14px;
    font-size: 13px;
    color: #B91C1C;
  }

  .reset-btn {
    background: none;
    border: none;
    font-size: 11.5px;
    color: var(--motm-text-muted);
    cursor: pointer;
    text-decoration: underline;
    padding: 0;
    font-family: inherit;
  }
  .reset-btn:hover { color: var(--motm-text); }

  .btn-row { display: flex; gap: 8px; }

  .preview-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 9px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    background: #fff;
    color: var(--motm-text);
    border: 1.5px solid var(--motm-border);
    cursor: pointer;
    transition: border-color 120ms;
  }
  .preview-btn:hover:not(:disabled) { border-color: var(--motm-pink); }
  .preview-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .download-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
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
  .download-btn.disabled { opacity: 0.5; pointer-events: none; }
</style>

<div class="page-header">
  <h1 class="page-title">Exports</h1>
  <p class="page-sub">Download production and logistics documents</p>
</div>

<div class="export-grid">
  {#each EXPORTS as exp, i}
    {@const state    = states[i]}
    {@const todayStr = todayEST()}
    {@const { from: wFrom, to: wTo } = weekBoundsEST(dates[i])}
    {@const { from: thisFrom }       = weekBoundsEST(todayStr)}
    {@const isThisWeek = wFrom === thisFrom}
    {@const isToday    = dates[i] === todayStr}

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

        <!-- Mode tabs -->
        <div class="mode-tabs">
          <button class="mode-tab" class:active={modes[i] === 'week'} onclick={() => setMode(i, 'week')}>Week</button>
          <button class="mode-tab" class:active={modes[i] === 'day'}  onclick={() => setMode(i, 'day')}>Day</button>
        </div>

        <!-- Shortcuts -->
        <div class="shortcuts">
          <button class="shortcut-btn" class:active={modes[i] === 'week' && isThisWeek} onclick={() => jumpThisWeek(i)}>This week</button>
          <button class="shortcut-btn" class:active={modes[i] === 'day'  && isToday}    onclick={() => jumpToday(i)}>Today</button>
        </div>

        <!-- Picker: arrows + clickable label -->
        <div class="picker">
          <button class="picker-nav" onclick={() => navigate(i, -1)}>‹</button>

          <Popover.Root bind:open={popoverOpen[i]}>
            <Popover.Trigger class="picker-label-btn">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="4" width="18" height="18" rx="2" ry="2"/>
                <line x1="16" y1="2" x2="16" y2="6"/>
                <line x1="8"  y1="2" x2="8"  y2="6"/>
                <line x1="3"  y1="10" x2="21" y2="10"/>
              </svg>
              {pickerLabel(i)}
            </Popover.Trigger>
            <Popover.Content class="motm-cal" align="center" sideOffset={6}>
              <Calendar
                type="single"
                value={toCalendarDate(dates[i])}
                onValueChange={(val) => onCalendarSelect(i, val)}
              />
            </Popover.Content>
          </Popover.Root>

          <button class="picker-nav" onclick={() => navigate(i, 1)}>›</button>
        </div>

        <!-- Preview result -->
        {#if state.phase === 'preview'}
          <div class="preview-box" transition:fade={{ duration: 150 }}>
            <p class="preview-label">
              {modes[i] === 'week'
                ? `Week of ${formatWeekDisplay(wFrom, wTo)}`
                : formatDateDisplay(dates[i])}
            </p>
            {#each previewLines(exp.endpoint, state.data) as line}
              <span class="preview-line">{line}</span>
            {/each}
            {#if previewLines(exp.endpoint, state.data).length === 0}
              <span class="preview-empty">No orders found.</span>
            {/if}
          </div>
          <button class="reset-btn" onclick={() => reset(i)}>← Change selection</button>
        {/if}

        {#if state.phase === 'error'}
          <div class="error-box" transition:fade={{ duration: 150 }}>
            {state.message}
          </div>
        {/if}

        <!-- Action buttons -->
        <div class="btn-row">
          {#if state.phase !== 'preview'}
            <button
              class="preview-btn"
              disabled={state.phase === 'loading'}
              onclick={() => fetchPreview(i, exp.endpoint)}
            >
              {state.phase === 'loading' ? 'Checking…' : 'Preview'}
            </button>
          {/if}

          <a
            href={state.phase === 'preview' ? buildUrl(i, exp.endpoint) : undefined}
            class="download-btn"
            class:disabled={state.phase !== 'preview'}
            data-sveltekit-reload
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <polyline points="8 17 12 21 16 17"/>
              <line x1="12" y1="12" x2="12" y2="21"/>
              <path d="M20.88 18.09A5 5 0 0 0 18 9h-1.26A8 8 0 1 0 3 16.29"/>
            </svg>
            Download
          </a>
        </div>

      </div>
    </div>
  {/each}
</div>
