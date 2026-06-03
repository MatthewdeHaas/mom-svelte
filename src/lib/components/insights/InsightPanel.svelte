<script lang="ts">
  import { fade } from 'svelte/transition';

  interface Props {
    title: string;
    loading?: boolean;
    error?: string | null;
    children?: import('svelte').Snippet;
  }

  let { title, loading = false, error = null, children }: Props = $props();
</script>

<div class="panel">
  <h3 class="panel-title">{title}</h3>
  {#if loading}
    <div class="skeleton" transition:fade={{ duration: 150 }}>
      <div class="skeleton-bar"></div>
      <div class="skeleton-bar short"></div>
      <div class="skeleton-bar medium"></div>
    </div>
  {:else if error}
    <div class="panel-error">{error}</div>
  {:else if children}
    {@render children()}
  {/if}
</div>

<style>
  .panel {
    background: var(--motm-surface);
    border: 1px solid var(--motm-border);
    border-radius: 12px;
    padding: 20px;
  }

  .panel-title {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--motm-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
    margin: 0 0 16px;
  }

  .skeleton {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 0;
  }

  .skeleton-bar {
    height: 14px;
    background: var(--motm-border);
    border-radius: 4px;
    animation: pulse 1.5s ease-in-out infinite;
  }

  .skeleton-bar.short { width: 40%; }
  .skeleton-bar.medium { width: 65%; }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }

  .panel-error {
    color: #9E4E66;
    font-size: 0.875rem;
    padding: 12px;
    background: #FDF2F8;
    border-radius: 6px;
  }
</style>
