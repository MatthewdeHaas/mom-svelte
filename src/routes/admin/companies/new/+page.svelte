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
    max-width: 560px;
  }
  .card-header {
    padding: 20px 24px 16px;
    border-bottom: 1px solid var(--motm-border);
  }
  .card-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--motm-text);
  }
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

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--motm-text);
    letter-spacing: 0.01em;
  }
  .label-optional {
    font-weight: 400;
    color: var(--motm-text-muted);
    margin-left: 4px;
  }
  input, textarea {
    padding: 8px 11px;
    border: 1px solid var(--motm-border);
    border-radius: 7px;
    font-size: 13.5px;
    font-family: inherit;
    color: var(--motm-text);
    background: #fff;
    outline: none;
    transition: border-color 120ms;
    resize: vertical;
  }
  input:focus, textarea:focus { border-color: var(--motm-pink); }
  input.error, textarea.error { border-color: #E24B4A; }
  .field-error {
    font-size: 12px;
    color: #A32D2D;
  }

  .btn {
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms;
  }
  .btn-primary {
    background: var(--motm-pink);
    color: #fff;
  }
  .btn-primary:hover { background: var(--motm-pink-dark); }
  .btn-ghost {
    background: transparent;
    color: var(--motm-text-muted);
    border: 1px solid var(--motm-border);
    text-decoration: none;
    display: inline-flex;
    align-items: center;
  }
  .btn-ghost:hover { background: #FAF8F6; color: var(--motm-text); }
</style>

<a href="/admin/companies" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Companies
</a>

<div style="margin-bottom: 24px;">
  <h1 style="font-size: 22px; font-weight: 600; color: var(--motm-text); letter-spacing: -0.02em;">New company</h1>
  <p style="font-size: 13.5px; color: var(--motm-text-muted); margin-top: 3px;">Add a new client company to the system</p>
</div>

<div class="card">
  <div class="card-header">
    <p class="card-title">Company details</p>
  </div>

  <form method="POST" use:enhance>
    <div class="card-body">
      <div class="field">
        <label for="name">Company name</label>
        <input
          id="name"
          name="name"
          type="text"
          class:error={form?.errors?.name}
          value={form?.values?.name ?? ''}
          placeholder="Acme Corp"
          required
          autocomplete="organization"
        />
        {#if form?.errors?.name}
          <span class="field-error">{form.errors.name}</span>
        {/if}
      </div>

      <div class="field">
        <label for="contact_email">
          Contact email
          <span class="label-optional">(optional)</span>
        </label>
        <input
          id="contact_email"
          name="contact_email"
          type="email"
          value={form?.values?.contact_email ?? ''}
          placeholder="orders@acme.com"
          autocomplete="email"
        />
      </div>

      <div class="field">
        <label for="phone">
          Phone
          <span class="label-optional">(optional)</span>
        </label>
        <input
          id="phone"
          name="phone"
          type="tel"
          value={form?.values?.phone ?? ''}
          placeholder="+1 555 000 0000"
          autocomplete="tel"
        />
      </div>

      <div class="field">
        <label for="shipping_address">
          Shipping address
          <span class="label-optional">(optional)</span>
        </label>
        <textarea
          id="shipping_address"
          name="shipping_address"
          rows="3"
          placeholder="123 Main St&#10;Vancouver, BC"
        >{form?.values?.shipping_address ?? ''}</textarea>
      </div>
    </div>

    <div class="card-footer">
      <a href="/admin/companies" class="btn btn-ghost">Cancel</a>
      <button type="submit" class="btn btn-primary">Create company</button>
    </div>
  </form>
</div>
