<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData, ActionData } from './$types';

  const { data, form }: { data: PageData; form: ActionData } = $props();
</script>

<style>
  h1 { font-size: 22px; font-weight: 600; color: #2C1F24; letter-spacing: -0.02em; margin: 0 0 4px; }
  .sub { font-size: 14px; color: #7A6670; margin: 0 0 28px; }

  .layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
    align-items: start;
  }
  @media (max-width: 700px) { .layout { grid-template-columns: 1fr; } }

  .card {
    background: #fff;
    border: 1px solid #EDE8E3;
    border-radius: 10px;
  }
  .card-header {
    padding: 18px 22px 14px;
    border-bottom: 1px solid #EDE8E3;
  }
  .card-title { font-size: 14px; font-weight: 600; color: #2C1F24; }
  .card-sub { font-size: 12.5px; color: #7A6670; margin-top: 2px; }
  .card-body { padding: 22px; display: flex; flex-direction: column; gap: 16px; }
  .card-footer {
    padding: 14px 22px;
    border-top: 1px solid #EDE8E3;
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    align-items: center;
  }

  .field { display: flex; flex-direction: column; gap: 5px; }
  label { font-size: 12.5px; font-weight: 600; color: #2C1F24; }
  input {
    padding: 8px 11px;
    border: 1px solid #EDE8E3;
    border-radius: 7px;
    font-size: 13.5px;
    font-family: inherit;
    color: #2C1F24;
    background: #fff;
    outline: none;
    transition: border-color 120ms;
  }
  input:focus { border-color: #C9718A; }
  input.error { border-color: #E24B4A; }
  input:disabled { background: #FAF8F6; color: #7A6670; cursor: not-allowed; }
  .field-error { font-size: 12px; color: #A32D2D; }
  .read-only-value {
    font-size: 13.5px;
    color: #2C1F24;
    padding: 8px 11px;
    background: #FAF8F6;
    border: 1px solid #EDE8E3;
    border-radius: 7px;
  }
  .read-only-empty { color: #B8AEB3; }

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
  .btn-primary { background: #C9718A; color: #fff; }
  .btn-primary:hover { background: #b05e77; }

  .success-strip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: #D1FAE5;
    color: #1F6648;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 500;
    margin-bottom: 16px;
  }

  .hint { font-size: 12px; color: #7A6670; margin-top: 2px; }
</style>

<h1>Account settings</h1>
<p class="sub">Manage your profile and view your company details.</p>

{#if form?.action === 'update' && form?.success}
  <div class="success-strip">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M20 6 9 17l-5-5"/>
    </svg>
    Profile updated
  </div>
{/if}

<div class="layout">
  <!-- Profile -->
  <div class="card">
    <div class="card-header">
      <p class="card-title">Profile</p>
      <p class="card-sub">Your name is shown to your account manager.</p>
    </div>
    <form method="POST" action="?/update" use:enhance>
      <div class="card-body">
        <div class="field">
          <label for="name">Display name</label>
          <input
            id="name"
            name="name"
            type="text"
            value={form?.values?.name ?? data.user.name}
            class:error={form?.errors?.name}
            required
          />
          {#if form?.errors?.name}
            <span class="field-error">{form.errors.name}</span>
          {/if}
        </div>

        <div class="field">
          <label>Email</label>
          <input type="email" value={data.user.email} disabled />
          <p class="hint">Email cannot be changed — sign in uses a magic link sent to this address.</p>
        </div>
      </div>
      <div class="card-footer">
        <button type="submit" class="btn btn-primary">Save changes</button>
      </div>
    </form>
  </div>

  <!-- Company (read-only) -->
  <div class="card">
    <div class="card-header">
      <p class="card-title">Company</p>
      <p class="card-sub">Managed by your account manager.</p>
    </div>
    <div class="card-body">
      <div class="field">
        <label>Company name</label>
        <div class="read-only-value">{data.company?.name ?? '—'}</div>
      </div>
      <div class="field">
        <label>Contact email</label>
        <div class="read-only-value" class:read-only-empty={!data.company?.contact_email}>
          {data.company?.contact_email ?? '—'}
        </div>
      </div>
      <div class="field">
        <label>Phone</label>
        <div class="read-only-value" class:read-only-empty={!data.company?.phone}>
          {data.company?.phone ?? '—'}
        </div>
      </div>
      <div class="field">
        <label>Shipping address</label>
        <div class="read-only-value" class:read-only-empty={!data.company?.shipping_address}
             style="white-space: pre-line;">
          {data.company?.shipping_address ?? '—'}
        </div>
      </div>
    </div>
  </div>
</div>
