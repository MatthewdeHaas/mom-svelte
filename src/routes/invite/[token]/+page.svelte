<script lang="ts">
  import type { PageData } from './$types';
  const { data }: { data: PageData } = $props();
</script>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 60vh;
    text-align: center;
    padding: 32px 20px;
    font-family: 'DM Sans', system-ui, sans-serif;
  }
  .icon {
    width: 56px; height: 56px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    margin-bottom: 20px;
  }
  .icon-warn  { background: #FEF3C7; }
  .icon-error { background: #FEE2E2; }
  .icon-warn  svg { color: #92400E; }
  .icon-error svg { color: #991B1B; }
  h1 { font-size: 20px; font-weight: 600; color: #2C1F24; margin: 0 0 10px; }
  p  { font-size: 14px; color: #7A6670; line-height: 1.6; max-width: 380px; margin: 0; }
  .contact { margin-top: 20px; font-size: 13px; color: #7A6670; }
  .contact a { color: #C9718A; text-decoration: none; font-weight: 500; }
  .contact a:hover { text-decoration: underline; }
</style>

<div class="wrap">
  {#if data.status === 'used'}
    <div class="icon icon-warn">
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"/><path d="M12 8v4m0 4h.01"/>
      </svg>
    </div>
    <h1>Invite already used</h1>
    <p>This invite link has already been claimed. If you need a new one, contact your account manager.</p>
  {:else if data.status === 'expired'}
    <div class="icon icon-warn">
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>
      </svg>
    </div>
    <h1>Invite expired</h1>
    <p>This invite link has expired (invites are valid for 48 hours). Ask your account manager to send a new one.</p>
  {:else if data.status === 'conflict'}
    <div class="icon icon-error">
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"/><path d="M12 8v4m0 4h.01"/>
      </svg>
    </div>
    <h1>Account conflict</h1>
    <p>Your account is already linked to a different company. Contact your account manager to resolve this.</p>
  {/if}

  <p class="contact">
    Questions? Email <a href="mailto:sam@mealsonthemove.ca">sam@mealsonthemove.ca</a>
  </p>
</div>
