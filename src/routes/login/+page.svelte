<script lang="ts">
  import { authClient } from '$lib/auth-client';

  let email = $state('');
  let error = $state('');
  let sent = $state(false);
  let loading = $state(false);

  const handleSubmit = async () => {
    error = '';
    loading = true;
    const { error: err } = await authClient.signIn.magicLink({ email, callbackURL: '/admin/dashboard' });
    if (err) {
      error = err.message ?? 'Something went wrong.';
    } else {
      sent = true;
    }
    loading = false;
  }
</script>

{#if sent}
  <p>Check your email for a sign-in link.</p>
{:else}
  <form onsubmit={handleSubmit}>
    <label>
      Email
      <input type="email" bind:value={email} required autocomplete="email" />
    </label>
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <button type="submit" disabled={loading}>
      {loading ? 'Sending…' : 'Send sign-in link'}
    </button>
  </form>
{/if}
