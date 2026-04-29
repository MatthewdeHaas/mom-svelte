<script lang="ts">
  import type { ActionData } from './$types';
  import { enhance } from '$app/forms';

  const { form }: { form: ActionData } = $props();
</script>

<style>
  .back-link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 13px;
    color: var(--motm-text-muted);
    text-decoration: none;
    margin-bottom: 20px;
    transition: color 120ms;
  }
  .back-link:hover { color: var(--motm-text); }

  .card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    max-width: 520px;
  }
  .card-header {
    padding: 20px 24px 16px;
    border-bottom: 1px solid var(--motm-border);
  }
  .card-title { font-size: 15px; font-weight: 600; color: var(--motm-text); }
  .card-sub { font-size: 12.5px; color: var(--motm-text-muted); margin-top: 3px; }
  .card-body {
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .card-footer {
    padding: 16px 24px;
    border-top: 1px solid var(--motm-border);
    display: flex;
    gap: 10px;
    justify-content: flex-end;
  }

  .field { display: flex; flex-direction: column; gap: 5px; }
  .field-row { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  label { font-size: 12.5px; font-weight: 600; color: var(--motm-text); }
  .label-optional { font-weight: 400; color: var(--motm-text-muted); margin-left: 4px; }
  .field-hint { font-size: 12px; color: var(--motm-text-muted); }
  .field-error { font-size: 12px; color: #A32D2D; }

  input {
    padding: 8px 11px;
    border: 1px solid var(--motm-border);
    border-radius: 7px;
    font-size: 13.5px;
    font-family: inherit;
    color: var(--motm-text);
    background: #fff;
    outline: none;
    transition: border-color 120ms;
  }
  input:focus { border-color: var(--motm-pink); }
  input.error { border-color: #E24B4A; }

  .btn {
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms;
    text-decoration: none;
    display: inline-flex;
    align-items: center;
  }
  .btn-primary { background: var(--motm-pink); color: #fff; }
  .btn-primary:hover { background: var(--motm-pink-dark); }
  .btn-ghost {
    background: transparent;
    color: var(--motm-text-muted);
    border: 1px solid var(--motm-border);
  }
  .btn-ghost:hover { background: #FAF8F6; color: var(--motm-text); }
</style>

<a href="/admin/menus" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Menus
</a>

<div style="margin-bottom: 24px;">
  <h1 style="font-size: 22px; font-weight: 600; color: var(--motm-text); letter-spacing: -0.02em;">New menu</h1>
  <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">You'll assign recipes to the cycle grid after creation</p>
</div>

<div class="card">
  <div class="card-header">
    <p class="card-title">Menu details</p>
    <p class="card-sub">The date range defines when this menu is active. The 4-week cycle repeats within that range.</p>
  </div>

  <form method="POST" use:enhance>
    <div class="card-body">
      <div class="field">
        <label for="name">Name</label>
        <input
          id="name"
          name="name"
          type="text"
          class:error={form?.errors?.name}
          value={form?.values?.name ?? ''}
          placeholder="e.g. Summer 2026"
          required
          autofocus
        />
        {#if form?.errors?.name}
          <span class="field-error">{form.errors.name}</span>
        {/if}
      </div>

      <div class="field-row">
        <div class="field">
          <label for="start_date">
            Start date
            <span class="label-optional">(optional)</span>
          </label>
          <input
            id="start_date"
            name="start_date"
            type="date"
            class:error={form?.errors?.start_date}
            value={form?.values?.start_date ?? ''}
          />
          {#if form?.errors?.start_date}
            <span class="field-error">{form.errors.start_date}</span>
          {/if}
        </div>

        <div class="field">
          <label for="end_date">
            End date
            <span class="label-optional">(optional)</span>
          </label>
          <input
            id="end_date"
            name="end_date"
            type="date"
            class:error={form?.errors?.end_date}
            value={form?.values?.end_date ?? ''}
          />
          {#if form?.errors?.end_date}
            <span class="field-error">{form.errors.end_date}</span>
          {/if}
        </div>
      </div>

      <p class="field-hint">
        Leaving dates blank creates an open-ended menu with no fixed schedule bounds.
      </p>
    </div>

    <div class="card-footer">
      <a href="/admin/menus" class="btn btn-ghost">Cancel</a>
      <button type="submit" class="btn btn-primary">Create and assign recipes →</button>
    </div>
  </form>
</div>
