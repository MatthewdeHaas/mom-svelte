<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { Separator } from '$lib/components/ui/separator';
  import { Badge } from '$lib/components/ui/badge';
  import { Sheet, SheetContent, SheetTrigger } from '$lib/components/ui/sheet';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import { Tooltip, TooltipContent, TooltipTrigger } from '$lib/components/ui/tooltip';

  // Icon imports
  import {
    LayoutDashboard,
    ShoppingBag,
    Building2,
    BookOpen,
    UtensilsCrossed,
    Carrot,
    ShieldAlert,
    Download,
    BarChart3,
    DollarSign,
    Flag,
    Menu,
    LogOut,
    ChevronRight,
  } from '@lucide/svelte';

  type NavItem = {
    label: string;
    href: string;
    icon: typeof LayoutDashboard;
    badge?: string;
  };

  type NavSection = {
    title: string;
    items: NavItem[];
  };

  const nav: NavSection[] = [
    {
      title: 'Overview',
      items: [
        { label: 'Dashboard', href: '/admin/dashboard', icon: LayoutDashboard },
      ],
    },
    {
      title: 'Operations',
      items: [
        { label: 'Orders',      href: '/admin/orders',      icon: ShoppingBag },
        { label: 'Companies',   href: '/admin/companies',   icon: Building2 },
      ],
    },
    {
      title: 'Catalogue',
      items: [
        { label: 'Menus',       href: '/admin/menus',       icon: BookOpen },
        { label: 'Recipes',     href: '/admin/recipes',     icon: UtensilsCrossed },
        { label: 'Ingredients', href: '/admin/ingredients', icon: Carrot },
        { label: 'Allergens',   href: '/admin/allergens',   icon: ShieldAlert },
      ],
    },
    {
      title: 'Data',
      items: [
        { label: 'Insights',    href: '/admin/insights',    icon: BarChart3 },
        { label: 'Financials',  href: '/admin/financials',  icon: DollarSign },
        { label: 'Exports',     href: '/admin/exports',     icon: Download },
        // Read-only drift mirror; the Rust endpoint behind it 403s for
        // everyone except the operator user id, so for every other admin
        // this surface returns 404 and the sidebar link dead-ends on a
        // Not Found page. That's intentional — flipping flags is a
        // local-TUI-plus-migration workflow, not something to expose in
        // the web admin. See `docs/feature-flags.md`.
        { label: 'Feature flags', href: '/admin/flags',     icon: Flag },
      ],
    },
  ];

  function isActive(href: string): boolean {
    return $page.url.pathname === href || $page.url.pathname.startsWith(href + '/');
  }

  function isRootActive(href: string): boolean {
    // Dashboard exact match; others prefix match
    if (href === '/admin/dashboard') return $page.url.pathname === href;
    return $page.url.pathname.startsWith(href);
  }

  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();

  let mobileOpen = $state(false);
</script>

<!-- ───────────────────────── CSS custom properties ───────────────────────── -->
<style>
  @import url('https://fonts.googleapis.com/css2?family=DM+Sans:wght@400;450;500;600&display=swap');

  :root {
    /* Warm off-white base — never pure #fff */
    --motm-bg:         #FAF8F6;
    --motm-surface:    #FFFFFF;
    --motm-border:     #EDE8E3;

    /* Sidebar: deep plum grounds the palette */
    --motm-sidebar-bg: #2E1521;
    --motm-sidebar-border: rgba(255,255,255,0.06);
    --motm-sidebar-text:   rgba(255,255,255,0.55);
    --motm-sidebar-hover:  rgba(255,255,255,0.07);
    --motm-sidebar-active-bg:   rgba(201,113,138,0.18);
    --motm-sidebar-active-text: #F0A8BB;

    /* Brand pink — desaturated, dusty rose */
    --motm-pink:       #C9718A;
    --motm-pink-light: #F5DCE4;
    --motm-pink-dark:  #9E4E66;

    /* Warm neutrals */
    --motm-text:       #2C1F24;
    --motm-text-muted: #7A6670;

    --motm-sidebar-width: 220px;
  }

  /* ── Shell layout ─────────────────────────────────── */
  .shell {
    display: flex;
    min-height: 100dvh;
    background: var(--motm-bg);
    font-family: 'DM Sans', system-ui, sans-serif;
  }

  /* ── Sidebar ──────────────────────────────────────── */
  .sidebar {
    width: var(--motm-sidebar-width);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    background: var(--motm-sidebar-bg);
    border-right: 1px solid var(--motm-sidebar-border);
    position: sticky;
    top: 0;
    height: 100dvh;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .sidebar::-webkit-scrollbar { display: none; }

  .sidebar-logo {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 20px 16px 16px;
    border-bottom: 1px solid var(--motm-sidebar-border);
    text-decoration: none;
  }
  .logo-mark {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: linear-gradient(135deg, var(--motm-pink) 0%, #9E4E66 100%);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .logo-mark svg {
    width: 16px;
    height: 16px;
    fill: white;
  }
  .logo-text {
    display: flex;
    flex-direction: column;
    line-height: 1.15;
  }
  .logo-name {
    font-size: 13px;
    font-weight: 600;
    color: #fff;
    letter-spacing: -0.01em;
  }
  .logo-sub {
    font-size: 10px;
    color: var(--motm-sidebar-text);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  /* nav sections */
  .nav-body {
    flex: 1;
    padding: 12px 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .nav-section {
    padding: 0 8px;
    margin-bottom: 8px;
  }
  .nav-section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--motm-sidebar-text);
    padding: 0 8px;
    margin-bottom: 2px;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 10px;
    border-radius: 7px;
    font-size: 13.5px;
    font-weight: 450;
    color: var(--motm-sidebar-text);
    text-decoration: none;
    transition: background 120ms, color 120ms;
    cursor: pointer;
    border: none;
    background: transparent;
    width: 100%;
    text-align: left;
  }
  .nav-item:hover {
    background: var(--motm-sidebar-hover);
    color: rgba(255,255,255,0.85);
  }
  .nav-item.active {
    background: var(--motm-sidebar-active-bg);
    color: var(--motm-sidebar-active-text);
    font-weight: 500;
  }
  .nav-item :global(svg) {
    width: 15px;
    height: 15px;
    flex-shrink: 0;
    opacity: 0.7;
  }
  .nav-item.active :global(svg) {
    opacity: 1;
  }

  .nav-divider {
    height: 1px;
    background: var(--motm-sidebar-border);
    margin: 6px 16px;
  }

  /* Logout at the bottom */
  .sidebar-footer {
    padding: 12px 16px;
    border-top: 1px solid var(--motm-sidebar-border);
  }
  .logout-btn {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 10px;
    border-radius: 7px;
    font-size: 13px;
    color: var(--motm-sidebar-text);
    background: transparent;
    border: none;
    cursor: pointer;
    width: 100%;
    text-align: left;
    transition: background 120ms, color 120ms;
  }
  .logout-btn:hover {
    background: rgba(255,255,255,0.05);
    color: rgba(255,255,255,0.7);
  }
  .logout-btn :global(svg) {
    width: 14px;
    height: 14px;
    opacity: 0.6;
  }

  /* ── Main content area ────────────────────────────── */
  .content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  /* Top bar — breadcrumb + page context */
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 28px;
    height: 52px;
    background: var(--motm-surface);
    border-bottom: 1px solid var(--motm-border);
    position: sticky;
    top: 0;
    z-index: 10;
  }
  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--motm-text-muted);
  }
  .breadcrumb-sep :global(svg) {
    width: 12px;
    height: 12px;
    opacity: 0.4;
  }
  .breadcrumb-current {
    color: var(--motm-text);
    font-weight: 500;
  }

  .topbar-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .env-badge {
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 5px;
    background: var(--motm-pink-light);
    color: var(--motm-pink-dark);
    letter-spacing: 0.04em;
  }

  /* Mobile hamburger — hidden on desktop */
  .mobile-trigger {
    display: none;
  }

  .page-slot {
    flex: 1;
    padding: 28px;
  }

  @media (max-width: 768px) {
    .sidebar { display: none; }
    .mobile-trigger { display: flex; }
    .topbar { padding: 0 16px; }
    .page-slot { padding: 16px; }
  }
</style>

<!-- ───────────────────────── Markup ──────────────────────────────────────── -->
<div class="shell">

  <!-- Desktop sidebar -->
  <aside class="sidebar">
    <a href="/admin/dashboard" class="sidebar-logo">
      <div class="logo-mark">
        <!-- Simple fork icon, inline SVG to avoid extra dep -->
        <svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg">
          <path d="M5 1v4a2 2 0 0 0 1.5 1.937V15h1V6.937A2 2 0 0 0 9 5V1H8v3.5a.5.5 0 0 1-1 0V1H6v3.5a.5.5 0 0 1-1 0V1H5zM11 1c-.552 0-1.5 1.343-1.5 3s.948 2.937 1.5 2.937V15h1V6.937C12.552 6.937 13.5 5.657 13.5 4S12.552 1 11 1z"/>
        </svg>
      </div>
      <div class="logo-text">
        <span class="logo-name">Meals on the Move</span>
        <span class="logo-sub">Admin</span>
      </div>
    </a>

    <nav class="nav-body">
      {#each nav as section, i}
        {#if i > 0}
          <div class="nav-divider"></div>
        {/if}
        <div class="nav-section">
          <p class="nav-section-title">{section.title}</p>
          {#each section.items as item}
            <a
              href={item.href}
              class="nav-item"
              class:active={isRootActive(item.href)}
            >
              <item.icon />
              {item.label}
              {#if item.badge}
                <span style="margin-left: auto; font-size: 11px; background: var(--motm-pink-light); color: var(--motm-pink-dark); padding: 1px 6px; border-radius: 4px;">
                  {item.badge}
                </span>
              {/if}
            </a>
          {/each}
        </div>
      {/each}
    </nav>

    <div class="sidebar-footer">
      <!--
        Form POST (not fetch) so the server-side form action receives
        application/x-www-form-urlencoded. A bare fetch with no body
        returns 415 Unsupported Media Type. Matches /client's pattern
        and keeps no-JS fallback working. Server action already issues
        a 302 redirect to /login — no client-side goto needed.
      -->
      <form method="POST" action="/logout">
        <button type="submit" class="logout-btn">
          <LogOut />
          Sign out
        </button>
      </form>
    </div>
  </aside>

  <!-- Main -->
  <div class="content">
    <header class="topbar">
      <!-- Mobile sheet trigger -->
      <div class="mobile-trigger">
        <Sheet bind:open={mobileOpen}>
          <SheetTrigger>
            {#snippet child({ props })}
              <button
                {...props}
                class={buttonVariants({ variant: 'ghost', size: 'icon' })}
                aria-label="Open menu"
              >
                <Menu class="h-5 w-5" />
              </button>
            {/snippet}
          </SheetTrigger>
          <SheetContent side="left" class="p-0 w-[220px]" style="background: var(--motm-sidebar-bg); border-right: 1px solid var(--motm-sidebar-border);">
            <!-- Re-use same nav structure for mobile sheet -->
            <nav style="padding: 16px 0;">
              {#each nav as section}
                <div style="padding: 0 8px; margin-bottom: 8px;">
                  <p class="nav-section-title">{section.title}</p>
                  {#each section.items as item}
                    <a
                      href={item.href}
                      class="nav-item"
                      class:active={isRootActive(item.href)}
                      onclick={() => (mobileOpen = false)}
                    >
                      <item.icon />
                      {item.label}
                    </a>
                  {/each}
                </div>
              {/each}
            </nav>
          </SheetContent>
        </Sheet>
      </div>

      <!-- Breadcrumb -->
      <div class="breadcrumb">
        <span>Admin</span>
        {#if $page.url.pathname !== '/admin/dashboard'}
          <span class="breadcrumb-sep"><ChevronRight /></span>
          <span class="breadcrumb-current">
            {$page.url.pathname.split('/').filter(Boolean).at(-1)?.replace(/-/g, ' ')}
          </span>
        {/if}
      </div>

      <div class="topbar-right">
        <span class="env-badge">Internal</span>
      </div>
    </header>

    <main class="page-slot">
      {@render children()}
    </main>
  </div>
</div>
