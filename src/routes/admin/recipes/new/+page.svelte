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
    max-width: 480px;
  }
  .card-header {
    padding: 20px 24px 16px;
    border-bottom: 1px solid var(--motm-border);
  }
  .card-title { font-size: 15px; font-weight: 600; color: var(--motm-text); }
  .card-sub { font-size: 12.5px; color: var(--motm-text-muted); margin-top: 3px; }
  .card-body { padding: 24px; }
  .card-footer {
    padding: 16px 24px;
    border-top: 1px solid var(--motm-border);
    display: flex;
    gap: 10px;
    justify-content: flex-end;
  }

  .field { display: flex; flex-direction: column; gap: 5px; }
  label { font-size: 12.5px; font-weight: 600; color: var(--motm-text); }
  .field-error { font-size: 12px; color: #A32D2D; }

  input[type="text"] {
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
  input[type="text"]:focus { border-color: var(--motm-pink); }
  input[type="text"].error { border-color: #E24B4A; }

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

<a href="/admin/recipes" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Recipes
</a>

<div style="margin-bottom: 24px;">
  <h1 style="font-size: 22px; font-weight: 600; color: var(--motm-text); letter-spacing: -0.02em;">New recipe</h1>
  <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">You can add ingredients after creating the recipe</p>
</div>

<div class="card">
  <div class="card-header">
    <p class="card-title">Recipe details</p>
    <p class="card-sub">Start with a name — ingredients are managed on the next screen</p>
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
          placeholder="e.g. Chicken Caesar Salad"
          required
        />
        {#if form?.errors?.name}
          <span class="field-error">{form.errors.name}</span>
        {/if}
      </div>
    </div>

    <div class="card-footer">
      <a href="/admin/recipes" class="btn btn-ghost">Cancel</a>
      <button type="submit" class="btn btn-primary">Create and add ingredients →</button>
    </div>
  </form>
</div>
