<script lang="ts">
  import { authClient } from '$lib/auth-client';

  let email = $state('');
  let error = $state('');
  let sent = $state(false);
  let loading = $state(false);

  const handleSubmit = async () => {
    error = '';
    loading = true;
    const { error: err } = await authClient.signIn.magicLink({
      email,
      callbackURL: '/admin/dashboard',
    });
    if (err) {
      error = err.message ?? 'Something went wrong.';
    } else {
      sent = true;
    }
    loading = false;
  };

  const handleResend = async () => {
    sent = false;
    error = '';
    loading = true;
    const { error: err } = await authClient.signIn.magicLink({
      email,
      callbackURL: '/admin/dashboard',
    });
    if (err) {
      error = err.message ?? 'Something went wrong.';
    } else {
      sent = true;
    }
    loading = false;
  };
</script>

<style>
  @import url('https://fonts.googleapis.com/css2?family=DM+Sans:wght@300;400;500;600&display=swap');

  :global(html, body) {
    margin: 0;
    padding: 0;
    min-height: 100dvh;
    font-family: 'DM Sans', system-ui, sans-serif;
  }

  .page {
    min-height: 100dvh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    background-color: #2E1521;
    background-image:
      radial-gradient(ellipse 80% 60% at 50% -10%, rgba(201,113,138,0.18) 0%, transparent 70%),
      radial-gradient(ellipse 50% 40% at 80% 100%, rgba(158,78,102,0.12) 0%, transparent 60%);
    padding: 32px 20px;
    box-sizing: border-box;
  }

  .logo {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    margin-bottom: 28px;
  }
  .logo-mark {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    background: linear-gradient(135deg, #C9718A 0%, #9E4E66 100%);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 0 0 1px rgba(255,255,255,0.08), 0 8px 20px rgba(0,0,0,0.3);
  }
  .logo-mark svg { width: 20px; height: 20px; fill: white; }
  .logo-name { font-size: 14px; font-weight: 600; color: #fff; letter-spacing: -0.01em; }
  .logo-tagline { font-size: 10.5px; color: rgba(255,255,255,0.35); letter-spacing: 0.08em; text-transform: uppercase; }

  .card {
    width: 100%;
    max-width: 400px;
    background: #fff;
    border-radius: 14px;
    box-shadow:
      0 0 0 1px rgba(0,0,0,0.06),
      0 24px 48px rgba(0,0,0,0.28),
      0 8px 16px rgba(0,0,0,0.16);
    overflow: hidden;
  }

  /* Input state */
  .card-body { padding: 32px 32px 28px; }
  .card-title {
    font-size: 19px;
    font-weight: 600;
    color: #2C1F24;
    letter-spacing: -0.02em;
    margin-bottom: 5px;
  }
  .card-sub {
    font-size: 13px;
    color: #7A6670;
    line-height: 1.55;
    margin-bottom: 24px;
  }
  .card-sub em { font-style: normal; color: #5A4650; font-weight: 500; }

  .field { display: flex; flex-direction: column; gap: 5px; margin-bottom: 12px; }
  label { font-size: 12px; font-weight: 600; color: #2C1F24; }

  input[type="email"] {
    padding: 10px 12px;
    border: 1.5px solid #EDE8E3;
    border-radius: 8px;
    font-size: 13.5px;
    font-family: inherit;
    color: #2C1F24;
    background: #FAF8F6;
    outline: none;
    transition: border-color 150ms, background 150ms;
    width: 100%;
    box-sizing: border-box;
  }
  input[type="email"]:focus { border-color: #C9718A; background: #fff; }
  input[type="email"].has-error { border-color: #E24B4A; }
  input[type="email"]::placeholder { color: #B8AEB3; }

  .error-strip {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    background: #FCEBEB;
    border: 1px solid #F0CBCB;
    border-radius: 7px;
    font-size: 13px;
    color: #A32D2D;
    line-height: 1.45;
    margin-bottom: 14px;
  }
  .error-strip svg { flex-shrink: 0; margin-top: 1px; }

  .submit-btn {
    width: 100%;
    padding: 10px 16px;
    border-radius: 9px;
    font-size: 13.5px;
    font-weight: 600;
    font-family: inherit;
    background: #C9718A;
    color: #fff;
    border: none;
    cursor: pointer;
    transition: background 150ms, opacity 150ms;
    margin-top: 2px;
  }
  .submit-btn:hover:not(:disabled) { background: #9E4E66; }
  .submit-btn:disabled { opacity: 0.55; cursor: not-allowed; }

  /* Sent state */
  .sent-body { padding: 32px 32px 28px; text-align: center; }
  .sent-icon {
    width: 52px;
    height: 52px;
    border-radius: 50%;
    background: #D1FAE5;
    display: flex;
    align-items: center;
    justify-content: center;
    margin: 0 auto 18px;
  }
  .sent-icon svg { width: 22px; height: 22px; color: #1F6648; }
  .sent-title {
    font-size: 18px;
    font-weight: 600;
    color: #2C1F24;
    letter-spacing: -0.02em;
    margin-bottom: 8px;
  }
  .sent-sub { font-size: 13px; color: #7A6670; line-height: 1.6; }
  .sent-email { font-size: 13.5px; font-weight: 600; color: #2C1F24; margin: 4px 0 8px; }

  .sent-divider { height: 1px; background: #EDE8E3; margin: 22px 0; }
  .resend-note { font-size: 12.5px; color: #7A6670; margin-bottom: 10px; }
  .resend-btn {
    width: 100%;
    padding: 9px 16px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    background: transparent;
    color: #2C1F24;
    border: 1.5px solid #EDE8E3;
    cursor: pointer;
    transition: border-color 150ms, background 150ms;
  }
  .resend-btn:hover:not(:disabled) { border-color: #C9718A; background: #FDF5F7; }
  .resend-btn:disabled { opacity: 0.55; cursor: not-allowed; }

  /* Footer */
  .card-footer { padding: 12px 32px; border-top: 1px solid #EDE8E3; background: #FAF8F6; }
  .footer-note { font-size: 11.5px; color: #B8AEB3; text-align: center; line-height: 1.5; }
  .footer-note a { color: #7A6670; text-decoration: none; font-weight: 500; }
  .footer-note a:hover { color: #2C1F24; }
</style>

<div class="page">
  <div class="logo">
    <div class="logo-mark">
      <svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg">
        <path d="M5 1v4a2 2 0 0 0 1.5 1.937V15h1V6.937A2 2 0 0 0 9 5V1H8v3.5a.5.5 0 0 1-1 0V1H6v3.5a.5.5 0 0 1-1 0V1H5zM11 1c-.552 0-1.5 1.343-1.5 3s.948 2.937 1.5 2.937V15h1V6.937C12.552 6.937 13.5 5.657 13.5 4S12.552 1 11 1z"/>
      </svg>
    </div>
    <div style="display: flex; flex-direction: column; align-items: center; gap: 1px;">
      <span class="logo-name">Meals on the Move</span>
      <span class="logo-tagline">Client Portal</span>
    </div>
  </div>

  <div class="card">
    {#if sent}
      <div class="sent-body">
        <div class="sent-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
            <path d="M22 2 11 13M22 2 15 22l-4-9-9-4 20-7z"/>
          </svg>
        </div>
        <p class="sent-title">Check your inbox</p>
        <p class="sent-sub">We sent a sign-in link to</p>
        <p class="sent-email">{email}</p>
        <p class="sent-sub" style="font-size: 12.5px;">
          The link expires shortly. Check your spam folder if it doesn't arrive.
        </p>
        <div class="sent-divider"></div>
        <p class="resend-note">Didn't receive it?</p>
        <button class="resend-btn" onclick={handleResend} disabled={loading}>
          {loading ? 'Sending…' : 'Resend link'}
        </button>
      </div>

    {:else}
      <div class="card-body">
        <p class="card-title">Sign in</p>
        <p class="card-sub">
          Enter your email and we'll send you a sign-in link.<br/>
          Access is <em>by invitation only</em>.
        </p>

        {#if error}
          <div class="error-strip">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10"/><path d="M12 8v4m0 4h.01"/>
            </svg>
            {error}
          </div>
        {/if}

        <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
          <div class="field">
            <label for="email">Email address</label>
            <input
              id="email"
              type="email"
              bind:value={email}
              class:has-error={!!error}
              placeholder="you@yourcompany.com"
              autocomplete="email"
              required
              spellcheck="false"
            />
          </div>
          <button type="submit" class="submit-btn" disabled={loading}>
            {loading ? 'Sending…' : 'Send sign-in link'}
          </button>
        </form>
      </div>
    {/if}

    <div class="card-footer">
      <p class="footer-note">
        Don't have access? Contact your account manager at
        <a href="mailto:sam@mealsonthemove.ca">sam@mealsonthemove.ca</a>
      </p>
    </div>
  </div>
</div>
