<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';

  interface Props {
    page: number;
    totalPages: number;
    total: number;
    perPage: number;
  }

  const props: Props = $props();

  const PER_PAGE_OPTIONS = [25, 50, 100];

  const rangeStart = $derived((props.page - 1) * props.perPage + 1);
  const rangeEnd = $derived(Math.min(props.page * props.perPage, props.total));

  function navigateToPage(newPage: number) {
    const params = new URLSearchParams($page.url.searchParams);
    if (newPage <= 1) params.delete('page');
    else params.set('page', String(newPage));
    goto(`?${params.toString()}`, { replaceState: true });
  }

  function changePerPage(e: Event) {
    const val = (e.target as HTMLSelectElement).value;
    const params = new URLSearchParams($page.url.searchParams);
    params.set('per_page', val);
    params.delete('page'); // reset to page 1
    goto(`?${params.toString()}`, { replaceState: true });
  }
</script>

<style>
  .pagination {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    font-size: 12.5px;
    color: var(--motm-text-muted);
    flex-wrap: wrap;
    gap: 10px;
  }
  .pagination-info {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .pagination-nav {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .page-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 30px;
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--motm-border);
    border-radius: 6px;
    font-size: 12.5px;
    font-family: inherit;
    font-weight: 500;
    background: #fff;
    color: var(--motm-text);
    cursor: pointer;
    transition: border-color 120ms, background 120ms;
  }
  .page-btn:hover:not(:disabled) {
    border-color: var(--motm-pink);
    background: #FEFBFC;
  }
  .page-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .page-btn.active {
    background: var(--motm-pink);
    border-color: var(--motm-pink);
    color: #fff;
  }
  .page-ellipsis {
    padding: 0 4px;
    color: var(--motm-text-muted);
  }
  .per-page-select {
    padding: 4px 6px;
    border: 1px solid var(--motm-border);
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    color: var(--motm-text);
    background: #fff;
    cursor: pointer;
  }
  .per-page-select:focus {
    border-color: var(--motm-pink);
    outline: none;
  }
</style>

{#if props.total > 0}
  <div class="pagination" data-testid="pagination">
    <div class="pagination-info">
      <span>Showing {rangeStart}–{rangeEnd} of {props.total}</span>
      <span>
        <select class="per-page-select" onchange={changePerPage}>
          {#each PER_PAGE_OPTIONS as opt}
            <option value={String(opt)} selected={opt === props.perPage}>{opt} / page</option>
          {/each}
        </select>
      </span>
    </div>

    <nav class="pagination-nav" aria-label="Pagination">
      <button
        class="page-btn"
        disabled={props.page <= 1}
        onclick={() => navigateToPage(props.page - 1)}
        aria-label="Previous page"
      >
        &lsaquo;
      </button>

      {#each pageNumbers(props.page, props.totalPages) as num}
        {#if num === -1}
          <span class="page-ellipsis">&hellip;</span>
        {:else}
          <button
            class="page-btn"
            class:active={num === props.page}
            onclick={() => navigateToPage(num)}
            aria-label="Page {num}"
            aria-current={num === props.page ? 'page' : undefined}
          >
            {num}
          </button>
        {/if}
      {/each}

      <button
        class="page-btn"
        disabled={props.page >= props.totalPages}
        onclick={() => navigateToPage(props.page + 1)}
        aria-label="Next page"
      >
        &rsaquo;
      </button>
    </nav>
  </div>
{/if}

<script lang="ts" module>
  /**
   * Build an array of page numbers to display, with -1 representing ellipsis.
   * Shows at most 7 page buttons: first, last, current, and neighbors.
   */
  export function pageNumbers(current: number, total: number): number[] {
    if (total <= 7) {
      return Array.from({ length: total }, (_, i) => i + 1);
    }

    const pages: number[] = [];
    pages.push(1);

    if (current > 3) pages.push(-1);

    const start = Math.max(2, current - 1);
    const end = Math.min(total - 1, current + 1);

    for (let i = start; i <= end; i++) {
      pages.push(i);
    }

    if (current < total - 2) pages.push(-1);

    pages.push(total);

    return pages;
  }
</script>
