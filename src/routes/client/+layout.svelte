<script lang="ts">
  import { page } from '$app/stores';

  let { children, data } = $props();

  const navItems = [
    { label: 'Dashboard',  href: '/client' },
    { label: 'Menu',       href: '/client/menu' },
    { label: 'Orders',     href: '/client/orders' },
    { label: 'Attendance', href: '/client/attendance' },
    { label: 'Roster',     href: '/client/roster' },
    { label: 'Settings',   href: '/client/settings' },
  ];
</script>

<style>
  @import url('https://fonts.googleapis.com/css2?family=DM+Sans:wght@300;400;500;600&display=swap');

  :global(html, body) {
    margin: 0;
    padding: 0;
    min-height: 100dvh;
    font-family: 'DM Sans', system-ui, sans-serif;
    background: #FAF8F6;
    color: #2C1F24;
  }

  .shell {
    display: flex;
    flex-direction: column;
    min-height: 100dvh;
  }

  /* Top bar */
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 24px;
    height: 56px;
    background: #2E1521;
    gap: 24px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    text-decoration: none;
  }
  .brand-mark {
    width: 28px;
    height: 28px;
    border-radius: 7px;
    background: linear-gradient(135deg, #C9718A 0%, #9E4E66 100%);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .brand-mark svg { width: 13px; height: 13px; fill: white; }
  .brand-name {
    font-size: 13px;
    font-weight: 600;
    color: #fff;
    white-space: nowrap;
  }

  nav {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: 1;
  }
  nav a {
    padding: 6px 12px;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 500;
    color: rgba(255,255,255,0.6);
    text-decoration: none;
    transition: background 120ms, color 120ms;
  }
  nav a:hover { background: rgba(255,255,255,0.07); color: rgba(255,255,255,0.9); }
  nav a.active { background: rgba(255,255,255,0.1); color: #fff; }

  .topbar-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .user-email {
    font-size: 12px;
    color: rgba(255,255,255,0.4);
    white-space: nowrap;
    display: none;
  }
  @media (min-width: 640px) { .user-email { display: block; } }

  .logout-btn {
    padding: 5px 12px;
    border-radius: 7px;
    font-size: 12.5px;
    font-weight: 500;
    font-family: inherit;
    background: transparent;
    color: rgba(255,255,255,0.55);
    border: 1px solid rgba(255,255,255,0.15);
    cursor: pointer;
    transition: background 120ms, color 120ms, border-color 120ms;
    white-space: nowrap;
  }
  .logout-btn:hover { background: rgba(255,255,255,0.07); color: #fff; border-color: rgba(255,255,255,0.3); }

  /* Main content */
  main {
    flex: 1;
    padding: 32px 24px;
    max-width: 960px;
    width: 100%;
    margin: 0 auto;
    box-sizing: border-box;
  }
</style>

<div class="shell">
  <header class="topbar">
    <a href="/client" class="brand">
      <div class="brand-mark">
        <svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg">
          <path d="M5 1v4a2 2 0 0 0 1.5 1.937V15h1V6.937A2 2 0 0 0 9 5V1H8v3.5a.5.5 0 0 1-1 0V1H6v3.5a.5.5 0 0 1-1 0V1H5zM11 1c-.552 0-1.5 1.343-1.5 3s.948 2.937 1.5 2.937V15h1V6.937C12.552 6.937 13.5 5.657 13.5 4S12.552 1 11 1z"/>
        </svg>
      </div>
      <span class="brand-name">Meals on the Move</span>
    </a>

    <nav>
      {#each navItems as item}
        <a
          href={item.href}
          class:active={$page.url.pathname === item.href}
        >{item.label}</a>
      {/each}
    </nav>

    <div class="topbar-right">
      <span class="user-email">{data.user?.email ?? ''}</span>
      <form method="POST" action="/logout">
        <button type="submit" class="logout-btn">Sign out</button>
      </form>
    </div>
  </header>

  <main>
    {@render children()}
  </main>
</div>
