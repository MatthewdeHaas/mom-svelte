<script lang="ts">
  import type { PageData, ActionData } from './$types';
  import { enhance } from '$app/forms';
  import type { OrderStatus } from '$lib/types';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  const statusConfig: Record<OrderStatus, { label: string; color: string; bg: string }> = {
    pending:   { label: 'Pending',   color: '#92600A', bg: '#FEF3C7' },
    confirmed: { label: 'Confirmed', color: '#1D6A8F', bg: '#DBEAFE' },
    fulfilled: { label: 'Fulfilled', color: '#1F6648', bg: '#D1FAE5' },
    cancelled: { label: 'Cancelled', color: '#6B4A4A', bg: '#F3E8E8' },
  };

  function formatDate(val: string | Date): string {
    return new Date(val).toLocaleDateString('en-GB', {
      day: 'numeric', month: 'short', year: 'numeric',
    });
  }

  function formatExpiry(val: string | Date): string {
    return new Date(val).toLocaleDateString('en-GB', {
      day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  }

  function inviteStatus(invite: { used_at: Date | null; expires_at: Date }): { label: string; color: string; bg: string } {
    if (invite.used_at) return { label: 'Used', color: '#1F6648', bg: '#D1FAE5' };
    if (new Date(invite.expires_at) < new Date()) return { label: 'Expired', color: '#6B4A4A', bg: '#F3E8E8' };
    return { label: 'Pending', color: '#92600A', bg: '#FEF3C7' };
  }

  // Surface the most recent 5 orders for the summary panel
  const recentOrders = $derived(data.orders.slice(0, 5));
  const activeInvites = $derived(data.invites.filter(i => !i.used_at && new Date(i.expires_at) >= new Date()));
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

  .page-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    margin-bottom: 28px;
    gap: 16px;
    flex-wrap: wrap;
  }
  .page-title {
    font-size: 22px;
    font-weight: 600;
    color: var(--motm-text);
    letter-spacing: -0.02em;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .inactive-chip {
    font-size: 12px;
    font-weight: 600;
    padding: 3px 9px;
    border-radius: 5px;
    background: #F3F3F1;
    color: #888;
    letter-spacing: 0.03em;
  }
  .page-sub {
    font-size: 13.5px;
    color: var(--motm-text-muted);
    margin-top: 3px;
  }

  /* Layout: main form + side summary */
  .layout {
    display: grid;
    grid-template-columns: 1fr 320px;
    gap: 20px;
    align-items: start;
  }
  @media (max-width: 900px) {
    .layout { grid-template-columns: 1fr; }
  }

  /* Cards */
  .card {
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 10px;
    margin-bottom: 20px;
  }
  .card-header {
    padding: 18px 22px 14px;
    border-bottom: 1px solid var(--motm-border);
  }
  .card-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--motm-text);
  }
  .card-sub {
    font-size: 12.5px;
    color: var(--motm-text-muted);
    margin-top: 2px;
  }
  .card-body {
    padding: 22px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .card-footer {
    padding: 14px 22px;
    border-top: 1px solid var(--motm-border);
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    align-items: center;
  }

  /* Form fields */
  .field { display: flex; flex-direction: column; gap: 5px; }
  .field-row { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--motm-text);
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
  input.error { border-color: #E24B4A; }
  input:disabled, textarea:disabled {
    background: #FAF8F6;
    color: var(--motm-text-muted);
    cursor: not-allowed;
  }
  .field-error { font-size: 12px; color: #A32D2D; }

  /* Buttons */
  .btn {
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    border: none;
    transition: background 120ms, color 120ms;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    text-decoration: none;
  }
  .btn-primary { background: var(--motm-pink); color: #fff; }
  .btn-primary:hover { background: var(--motm-pink-dark); }
  .btn-ghost {
    background: transparent;
    color: var(--motm-text-muted);
    border: 1px solid var(--motm-border);
  }
  .btn-ghost:hover { background: #FAF8F6; color: var(--motm-text); }
  .btn-danger {
    background: transparent;
    color: #A32D2D;
    border: 1px solid #F09595;
  }
  .btn-danger:hover { background: #FCEBEB; }

  /* Success toast strip */
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

  /* Danger zone */
  .danger-zone {
    border-color: #F0CBCB;
  }
  .danger-zone .card-header {
    border-bottom-color: #F0CBCB;
  }
  .danger-zone-body {
    padding: 22px;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 24px;
    flex-wrap: wrap;
  }
  .danger-desc {
    font-size: 13px;
    color: var(--motm-text-muted);
    max-width: 380px;
    line-height: 1.55;
  }

  .status-badge {
    display: inline-flex;
    padding: 2px 7px;
    border-radius: 4px;
    font-size: 11.5px;
    font-weight: 600;
  }

  .mini-table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
  .mini-table td { padding: 8px 0; border-bottom: 1px solid var(--motm-border); color: var(--motm-text); }
  .mini-table tr:last-child td { border-bottom: none; }
  .mini-table .td-muted { color: var(--motm-text-muted); }
  .mini-table .td-link { font-weight: 500; text-decoration: none; color: var(--motm-text); }
  .mini-table .td-link:hover { color: var(--motm-pink-dark); }

  .pill-list { display: flex; flex-wrap: wrap; gap: 6px; padding: 14px 0 6px; }
  .person-pill {
    padding: 3px 10px;
    border-radius: 20px;
    font-size: 12px;
    font-weight: 500;
    background: var(--motm-pink-light);
    color: var(--motm-pink-dark);
  }
  .empty-side {
    font-size: 12.5px;
    color: var(--motm-text-muted);
    padding: 12px 0;
  }

  .view-all-link {
    display: block;
    text-align: center;
    font-size: 12px;
    color: var(--motm-pink);
    text-decoration: none;
    padding-top: 10px;
    font-weight: 500;
  }
  .view-all-link:hover { color: var(--motm-pink-dark); text-decoration: underline; }

  /* Invite section */
  .invite-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .invite-row input {
    flex: 1;
  }
  .invite-history {
    margin-top: 16px;
  }
  .invite-history-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--motm-text-muted);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    margin-bottom: 8px;
  }
  .invite-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0;
    border-bottom: 1px solid var(--motm-border);
    gap: 12px;
  }
  .invite-item:last-child { border-bottom: none; }
  .invite-email { font-size: 13px; color: var(--motm-text); font-weight: 500; }
  .invite-expiry { font-size: 11.5px; color: var(--motm-text-muted); margin-top: 1px; }
  .invite-badge {
    padding: 2px 7px;
    border-radius: 4px;
    font-size: 11.5px;
    font-weight: 600;
    white-space: nowrap;
  }
  .invite-revoke-btn {
    padding: 4px 10px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    background: transparent;
    color: #A32D2D;
    border: 1px solid #F09595;
    cursor: pointer;
    transition: background 120ms;
    white-space: nowrap;
  }
  .invite-revoke-btn:hover { background: #FCEBEB; }
</style>

<a href="/admin/companies" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Companies
</a>

<div class="page-header">
  <div>
    <h1 class="page-title">
      {data.company.name}
      {#if !data.company.active}
        <span class="inactive-chip">Inactive</span>
      {/if}
    </h1>
    <p class="page-sub">Client since {formatDate(data.company.created_at)}</p>
  </div>
  <a href="/admin/orders?company_id={data.company.id}" class="btn btn-ghost">
    View all orders
  </a>
</div>

{#if form?.action === 'update' && form?.success}
  <div class="success-strip">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M20 6 9 17l-5-5"/>
    </svg>
    Changes saved
  </div>
{/if}

{#if form?.action === 'reactivate' && form?.success}
  <div class="success-strip">
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M20 6 9 17l-5-5"/>
    </svg>
    Company reactivated — you can now edit it
  </div>
{/if}

<div class="layout">
  <!-- Left: edit form + danger zone -->
  <div>
    <div class="card">
      <div class="card-header">
        <p class="card-title">Company details</p>
      </div>
      <form method="POST" action="?/update" use:enhance>
        <div class="card-body">
          <div class="field">
            <label for="name">Company name</label>
            <input
              id="name"
              name="name"
              type="text"
              value={form?.values?.name ?? data.company.name}
              class:error={form?.errors?.name}
              disabled={!data.company.active}
              required
            />
            {#if form?.errors?.name}
              <span class="field-error">{form.errors.name}</span>
            {/if}
          </div>

          <div class="field-row">
            <div class="field">
              <label for="contact_email">
                Contact email
                <span class="label-optional">(optional)</span>
              </label>
              <input
                id="contact_email"
                name="contact_email"
                type="email"
                value={form?.values?.contact_email ?? data.company.contact_email ?? ''}
                disabled={!data.company.active}
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
                value={form?.values?.phone ?? data.company.phone ?? ''}
                disabled={!data.company.active}
              />
            </div>
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
              disabled={!data.company.active}
            >{form?.values?.shipping_address ?? data.company.shipping_address ?? ''}</textarea>
          </div>
        </div>

        <div class="card-footer">
          {#if !data.company.active}
            <span style="font-size: 12.5px; color: var(--motm-text-muted);">Inactive companies cannot be edited</span>
          {:else}
            <button type="submit" class="btn btn-primary">Save changes</button>
          {/if}
        </div>
      </form>
      {#if !data.company.active}
        <div class="card-footer" style="border-top: 1px solid var(--motm-border); padding: 12px 18px;">
          <form method="POST" action="?/reactivate" use:enhance>
            <button type="submit" class="btn btn-primary">Reactivate company</button>
          </form>
        </div>
      {/if}
    </div>

    <!-- Invite users -->
    <div class="card" style="margin-bottom: 20px;">
      <div class="card-header">
        <p class="card-title">Invite a user</p>
        <p class="card-sub">Send an invite link to give someone client portal access for this company.</p>
      </div>
      <form method="POST" action="?/invite" use:enhance>
        <div class="card-body">
          {#if form?.action === 'invite' && form?.success}
            <div class="success-strip">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <path d="M20 6 9 17l-5-5"/>
              </svg>
              Invite sent
            </div>
          {/if}
          <div class="field">
            <label for="invite-email">Email address</label>
            <div class="invite-row">
              <input
                id="invite-email"
                name="email"
                type="email"
                placeholder="client@example.com"
                value={form?.action === 'invite' ? ((form?.values as Record<string, string>)?.email ?? '') : ''}
                class:error={form?.action === 'invite' && (form?.errors as Record<string, string>)?.email}
                required
              />
              <button type="submit" class="btn btn-primary">Send invite</button>
            </div>
            {#if form?.action === 'invite' && (form?.errors as Record<string, string>)?.email}
              <span class="field-error">{(form.errors as Record<string, string>).email}</span>
            {/if}
          </div>
        </div>
      </form>

      {#if data.invites.length > 0}
        <div class="card-body" style="padding-top: 0;">
          <div class="invite-history">
            <p class="invite-history-title">Invite history</p>
            {#each data.invites as invite}
              {@const status = inviteStatus(invite)}
              <div class="invite-item">
                <div>
                  <p class="invite-email">{invite.email}</p>
                  <p class="invite-expiry">
                    {#if invite.used_at}
                      Used {formatExpiry(invite.used_at)}
                    {:else}
                      Expires {formatExpiry(invite.expires_at)}
                    {/if}
                  </p>
                </div>
                <div style="display:flex;align-items:center;gap:8px;">
                  <span class="invite-badge" style="background:{status.bg};color:{status.color};">
                    {status.label}
                  </span>
                  {#if !invite.used_at && new Date(invite.expires_at) >= new Date()}
                    <form method="POST" action="?/revokeInvite" use:enhance>
                      <input type="hidden" name="invite_id" value={invite.id} />
                      <button type="submit" class="invite-revoke-btn">Revoke</button>
                    </form>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <!-- Danger zone -->
    {#if data.company.active}
      <div class="card danger-zone">
        <div class="card-header">
          <p class="card-title" style="color: #A32D2D;">Danger zone</p>
        </div>
        <div class="danger-zone-body">
          <p class="danger-desc">
            Deactivating this company will hide it from all active lists. Existing orders and history are preserved and remain fully accessible.
          </p>
          <form method="POST" action="?/deactivate" use:enhance>
            <button type="submit" class="btn btn-danger">
              Deactivate company
            </button>
          </form>
        </div>
      </div>
    {/if}
  </div>

  <!-- Right: summary panels -->
  <div>
    <!-- Order summary -->
    <div class="card" style="margin-bottom: 16px;">
      <div class="card-header">
        <p class="card-title">Recent orders</p>
        <p class="card-sub">{data.orders.length} total</p>
      </div>
      <div class="card-body" style="padding: 14px 18px;">
        {#if recentOrders.length === 0}
          <p class="empty-side">No orders yet.</p>
        {:else}
          <table class="mini-table">
            <tbody>
            {#each recentOrders as order}
              {@const status = statusConfig[order.status as OrderStatus]}
              <tr>
                <td>
                  <a href="/admin/orders/{order.id}" class="td-link">{order.recipe_name}</a>
                  <div class="td-muted" style="font-size: 11.5px; margin-top: 1px;">{formatDate(order.serving_date)}</div>
                </td>
                <td style="text-align: right;">
                  <span class="status-badge" style="background: {status.bg}; color: {status.color};">
                    {status.label}
                  </span>
                </td>
              </tr>
            {/each}
            </tbody>
          </table>
          {#if data.orders.length > 5}
            <a href="/admin/orders?company_id={data.company.id}" class="view-all-link">
              View all {data.orders.length} orders →
            </a>
          {/if}
        {/if}
      </div>
    </div>

    <!-- Persons -->
    <div class="card">
      <div class="card-header">
        <p class="card-title">Persons</p>
        <p class="card-sub">{data.persons.length} active</p>
      </div>
      <div class="card-body" style="padding: 4px 18px 14px;">
        {#if data.persons.length === 0}
          <p class="empty-side">No persons linked to this company.</p>
        {:else}
          <div class="pill-list">
            {#each data.persons as person}
              <span class="person-pill">{person.name}</span>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>
