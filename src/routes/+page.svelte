<script lang="ts">
  // No data needed — pure marketing page
</script>

<svelte:head>
  <title>Meals on the Move — Fresh meals delivered to care facilities</title>
  <meta name="description" content="Meals on the Move delivers fresh, allergen-conscious meals to childcare centres, residential care homes, and healthcare facilities across Ontario." />
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
  <link href="https://fonts.googleapis.com/css2?family=Playfair+Display:ital,wght@0,400;0,600;1,400&family=DM+Sans:wght@300;400;500;600&display=swap" rel="stylesheet" />
</svelte:head>

<style>
  :global(*, *::before, *::after) { box-sizing: border-box; margin: 0; padding: 0; }
  :global(html) { scroll-behavior: smooth; }
  :global(body) {
    font-family: 'DM Sans', system-ui, sans-serif;
    background: #FAF8F4;
    color: #2C1F24;
    -webkit-font-smoothing: antialiased;
  }

  /* ── Tokens ──────────────────────────────────── */
  :root {
    --plum:       #2E1521;
    --plum-mid:   #4A2535;
    --pink:       #C9718A;
    --pink-dark:  #9E4E66;
    --pink-light: #F5DCE4;
    --cream:      #FAF8F4;
    --stone:      #F0EBE3;
    --border:     #E8E0D6;
    --text:       #2C1F24;
    --muted:      #7A6670;
    --serif:      'Playfair Display', Georgia, serif;
    --sans:       'DM Sans', system-ui, sans-serif;
  }

  /* ── Nav ─────────────────────────────────────── */
  nav {
    position: sticky;
    top: 0;
    z-index: 50;
    background: rgba(250, 248, 244, 0.92);
    backdrop-filter: blur(12px);
    border-bottom: 1px solid var(--border);
    padding: 0 clamp(24px, 5vw, 80px);
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 60px;
  }
  .nav-logo {
    display: flex;
    align-items: center;
    gap: 10px;
    text-decoration: none;
  }
  .nav-logo-mark {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: linear-gradient(135deg, var(--pink), var(--pink-dark));
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .nav-logo-mark svg { width: 14px; height: 14px; fill: white; }
  .nav-logo-name {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text);
    letter-spacing: -0.01em;
  }
  .nav-cta {
    padding: 7px 16px;
    background: var(--plum);
    color: #fff;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    text-decoration: none;
    transition: background 150ms;
  }
  .nav-cta:hover { background: var(--plum-mid); }

  /* ── Sections shared ─────────────────────────── */
  section { padding: clamp(64px, 8vw, 120px) clamp(24px, 5vw, 80px); }
  .container { max-width: 1100px; margin: 0 auto; }

  /* ── Hero ────────────────────────────────────── */
  .hero {
    background: var(--plum);
    padding: clamp(80px, 10vw, 140px) clamp(24px, 5vw, 80px);
    position: relative;
    overflow: hidden;
  }
  .hero::before {
    content: '';
    position: absolute;
    top: -40%;
    right: -20%;
    width: 600px;
    height: 600px;
    border-radius: 50%;
    background: radial-gradient(circle, rgba(201,113,138,0.15) 0%, transparent 70%);
    pointer-events: none;
  }
  .hero::after {
    content: '';
    position: absolute;
    bottom: -30%;
    left: -10%;
    width: 400px;
    height: 400px;
    border-radius: 50%;
    background: radial-gradient(circle, rgba(201,113,138,0.08) 0%, transparent 70%);
    pointer-events: none;
  }
  .hero-inner {
    max-width: 1100px;
    margin: 0 auto;
    position: relative;
    z-index: 1;
  }
  .hero-eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--pink);
    margin-bottom: 28px;
  }
  .hero-eyebrow::before {
    content: '';
    display: block;
    width: 24px;
    height: 1.5px;
    background: var(--pink);
  }
  .hero-title {
    font-family: var(--serif);
    font-size: clamp(44px, 6vw, 80px);
    font-weight: 600;
    color: #fff;
    line-height: 1.1;
    letter-spacing: -0.02em;
    max-width: 700px;
    margin-bottom: 12px;
  }
  .hero-title em {
    font-style: italic;
    color: var(--pink);
  }
  .hero-subtitle {
    font-size: clamp(16px, 2vw, 19px);
    color: rgba(255,255,255,0.55);
    max-width: 520px;
    line-height: 1.6;
    margin-bottom: 40px;
    font-weight: 300;
  }
  .hero-actions {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
  }
  .btn-primary {
    padding: 13px 28px;
    background: var(--pink);
    color: #fff;
    border-radius: 10px;
    font-size: 14.5px;
    font-weight: 600;
    text-decoration: none;
    transition: background 150ms;
    font-family: var(--sans);
  }
  .btn-primary:hover { background: var(--pink-dark); }
  .btn-ghost-light {
    padding: 13px 28px;
    background: transparent;
    color: rgba(255,255,255,0.65);
    border: 1.5px solid rgba(255,255,255,0.2);
    border-radius: 10px;
    font-size: 14.5px;
    font-weight: 500;
    text-decoration: none;
    transition: border-color 150ms, color 150ms;
    font-family: var(--sans);
  }
  .btn-ghost-light:hover { border-color: rgba(255,255,255,0.5); color: #fff; }

  .hero-allergen {
    margin-top: 52px;
    padding-top: 32px;
    border-top: 1px solid rgba(255,255,255,0.1);
    display: flex;
    align-items: flex-start;
    gap: 16px;
    flex-wrap: wrap;
  }
  .allergen-label {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: rgba(255,255,255,0.35);
    padding-top: 2px;
    white-space: nowrap;
  }
  .allergen-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .allergen-tag {
    padding: 4px 10px;
    border: 1px solid rgba(255,255,255,0.15);
    border-radius: 20px;
    font-size: 12px;
    color: rgba(255,255,255,0.6);
    font-weight: 500;
  }

  /* ── Pillars ─────────────────────────────────── */
  .pillars {
    background: var(--stone);
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    padding: clamp(40px, 5vw, 64px) clamp(24px, 5vw, 80px);
  }
  .pillars-grid {
    max-width: 1100px;
    margin: 0 auto;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 0;
  }
  .pillar {
    padding: 28px 32px;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .pillar:last-child { border-right: none; }
  @media (max-width: 680px) {
    .pillar { border-right: none; border-bottom: 1px solid var(--border); }
    .pillar:last-child { border-bottom: none; }
  }
  .pillar-icon {
    width: 36px;
    height: 36px;
    border-radius: 9px;
    background: var(--pink-light);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .pillar-icon svg { width: 17px; height: 17px; color: var(--pink-dark); }
  .pillar-title {
    font-size: 14.5px;
    font-weight: 600;
    color: var(--text);
    line-height: 1.3;
  }

  /* ── About ───────────────────────────────────── */
  .about-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: clamp(40px, 5vw, 80px);
    align-items: start;
  }
  @media (max-width: 760px) { .about-grid { grid-template-columns: 1fr; } }
  .section-eyebrow {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--pink);
    margin-bottom: 16px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .section-eyebrow::before {
    content: '';
    display: block;
    width: 20px;
    height: 1.5px;
    background: var(--pink);
  }
  .section-title {
    font-family: var(--serif);
    font-size: clamp(30px, 3.5vw, 46px);
    font-weight: 600;
    color: var(--text);
    line-height: 1.15;
    letter-spacing: -0.02em;
    margin-bottom: 20px;
  }
  .section-title em { font-style: italic; color: var(--plum-mid); }
  .section-body {
    font-size: 15.5px;
    color: var(--muted);
    line-height: 1.7;
    font-weight: 300;
  }
  .section-body p + p { margin-top: 14px; }

  .about-stats {
    display: flex;
    flex-direction: column;
    gap: 0;
    padding-left: clamp(0px, 4vw, 48px);
    border-left: 1.5px solid var(--border);
  }
  .about-stat {
    padding: 28px 0;
    border-bottom: 1px solid var(--border);
  }
  .about-stat:last-child { border-bottom: none; }
  .about-stat-num {
    font-family: var(--serif);
    font-size: clamp(36px, 4vw, 52px);
    font-weight: 600;
    color: var(--plum);
    line-height: 1;
    letter-spacing: -0.03em;
    margin-bottom: 6px;
  }
  .about-stat-num em { color: var(--pink); font-style: normal; }
  .about-stat-label {
    font-size: 14px;
    font-weight: 500;
    color: var(--text);
    margin-bottom: 3px;
  }
  .about-stat-sub {
    font-size: 13px;
    color: var(--muted);
    line-height: 1.5;
    font-weight: 300;
  }

  /* ── Sectors ─────────────────────────────────── */
  .sectors { background: var(--cream); }
  .sectors-header { text-align: center; margin-bottom: clamp(40px, 5vw, 60px); }
  .sectors-header .section-eyebrow { justify-content: center; }
  .sectors-header .section-eyebrow::before { display: none; }
  .sectors-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 20px;
  }
  .sector-card {
    background: #fff;
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 32px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    transition: border-color 200ms, box-shadow 200ms;
  }
  .sector-card:hover {
    border-color: var(--pink);
    box-shadow: 0 4px 20px rgba(201,113,138,0.1);
  }
  .sector-icon {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    background: var(--pink-light);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .sector-icon svg { width: 21px; height: 21px; color: var(--pink-dark); }
  .sector-name {
    font-family: var(--serif);
    font-size: 20px;
    font-weight: 600;
    color: var(--text);
    letter-spacing: -0.01em;
  }
  .sector-desc {
    font-size: 14px;
    color: var(--muted);
    line-height: 1.65;
    font-weight: 300;
    flex: 1;
  }
  .sector-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .sector-tag {
    padding: 3px 10px;
    background: var(--stone);
    border-radius: 20px;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
  }

  /* ── Care section ────────────────────────────── */
  .care { background: var(--plum); }
  .care-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: clamp(40px, 5vw, 80px);
    align-items: center;
  }
  @media (max-width: 760px) { .care-grid { grid-template-columns: 1fr; } }
  .care .section-eyebrow { color: var(--pink); }
  .care .section-eyebrow::before { background: var(--pink); }
  .care .section-title { color: #fff; }
  .care .section-body { color: rgba(255,255,255,0.55); }
  .care-points { display: flex; flex-direction: column; gap: 20px; margin-top: 32px; }
  .care-point { display: flex; gap: 14px; align-items: flex-start; }
  .care-point-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--pink);
    flex-shrink: 0;
    margin-top: 7px;
  }
  .care-point-text { font-size: 14.5px; color: rgba(255,255,255,0.7); line-height: 1.6; font-weight: 300; }
  .care-point-text strong { color: #fff; font-weight: 500; }

  .care-visual {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .care-tile {
    background: rgba(255,255,255,0.06);
    border: 1px solid rgba(255,255,255,0.1);
    border-radius: 12px;
    padding: 24px 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .care-tile:first-child { grid-column: 1 / -1; background: rgba(201,113,138,0.15); border-color: rgba(201,113,138,0.25); }
  .care-tile-label { font-size: 12px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase; color: rgba(255,255,255,0.35); }
  .care-tile-value { font-family: var(--serif); font-size: 28px; font-weight: 600; color: #fff; letter-spacing: -0.02em; }
  .care-tile-sub { font-size: 12.5px; color: rgba(255,255,255,0.4); }

  /* ── Contact ─────────────────────────────────── */
  .contact { background: var(--cream); }
  .contact-header { text-align: center; margin-bottom: clamp(40px, 5vw, 56px); }
  .contact-header .section-eyebrow { justify-content: center; }
  .contact-header .section-eyebrow::before { display: none; }
  .contact-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 16px;
  }
  .contact-card {
    background: #fff;
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    transition: border-color 150ms;
  }
  .contact-card:hover { border-color: var(--pink); }
  .contact-role {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--pink);
    margin-bottom: 4px;
  }
  .contact-name {
    font-family: var(--serif);
    font-size: 19px;
    font-weight: 600;
    color: var(--text);
    margin-bottom: 10px;
  }
  .contact-detail {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    color: var(--muted);
    text-decoration: none;
    transition: color 120ms;
  }
  .contact-detail:hover { color: var(--text); }
  .contact-detail svg { width: 14px; height: 14px; flex-shrink: 0; opacity: 0.5; }
  .contact-card.order-desk {
    background: var(--plum);
    border-color: transparent;
  }
  .contact-card.order-desk .contact-role { color: var(--pink); }
  .contact-card.order-desk .contact-name { color: #fff; }
  .contact-card.order-desk .contact-detail { color: rgba(255,255,255,0.6); }
  .contact-card.order-desk .contact-detail:hover { color: #fff; }

  /* ── Footer ──────────────────────────────────── */
  footer {
    background: var(--plum);
    border-top: 1px solid rgba(255,255,255,0.06);
    padding: 32px clamp(24px, 5vw, 80px);
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
  }
  .footer-left { display: flex; flex-direction: column; gap: 4px; }
  .footer-logo { display: flex; align-items: center; gap: 8px; text-decoration: none; }
  .footer-logo-mark {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: linear-gradient(135deg, var(--pink), var(--pink-dark));
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .footer-logo-mark svg { width: 11px; height: 11px; fill: white; }
  .footer-logo-name { font-size: 13px; font-weight: 600; color: rgba(255,255,255,0.8); }
  .footer-address { font-size: 12px; color: rgba(255,255,255,0.3); margin-top: 2px; }
  .footer-copy { font-size: 12px; color: rgba(255,255,255,0.25); }
</style>

<!-- Nav -->
<nav>
  <a href="/" class="nav-logo">
		<img src="/mom-logo.svg" alt="Meals on the Move" width="64" height="64" />
    <span class="nav-logo-name">Meals on the Move</span>
  </a>
  <a href="#contact" class="nav-cta">Get in touch</a>
</nav>

<!-- Hero -->
<section class="hero">
  <div class="hero-inner">
    <div class="hero-eyebrow">Ontario's care meal specialists</div>
    <h1 class="hero-title">
      Better food.<br/>
      <em>More time for care.</em>
    </h1>
    <p class="hero-subtitle">
      Fresh, allergen-conscious meals cooked daily and delivered directly to childcare centres,
      residential care homes, and healthcare facilities across Ontario.
    </p>
    <div class="hero-actions">
      <a href="#contact" class="btn-primary">Get in touch</a>
      <a href="#sectors" class="btn-ghost-light">Who we serve</a>
    </div>

    <div class="hero-allergen">
      <span class="allergen-label">All meals are</span>
      <div class="allergen-tags">
        <span class="allergen-tag">Peanut free</span>
        <span class="allergen-tag">Tree nut free</span>
        <span class="allergen-tag">Sesame free</span>
        <span class="allergen-tag">Shellfish free</span>
      </div>
    </div>
  </div>
</section>

<!-- Pillars -->
<div class="pillars">
  <div class="pillars-grid">
    <div class="pillar">
      <div class="pillar-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
          <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z"/>
        </svg>
      </div>
      <p class="pillar-title">Great tasting meals</p>
    </div>
    <div class="pillar">
      <div class="pillar-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
          <circle cx="12" cy="12" r="10"/>
          <polyline points="12 6 12 12 16 14"/>
        </svg>
      </div>
      <p class="pillar-title">Cooked fresh & delivered same day</p>
    </div>
    <div class="pillar">
      <div class="pillar-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
          <path d="M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7z"/>
          <circle cx="12" cy="9" r="2.5"/>
        </svg>
      </div>
      <p class="pillar-title">Produced locally in Toronto</p>
    </div>
    <div class="pillar">
      <div class="pillar-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
          <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
          <polyline points="9 22 9 12 15 12 15 22"/>
        </svg>
      </div>
      <p class="pillar-title">Available hot or chilled</p>
    </div>
  </div>
</div>

<!-- About -->
<section>
  <div class="container">
    <div class="about-grid">
      <div>
        <div class="section-eyebrow">About us</div>
        <h2 class="section-title">Ontario's trusted <em>care meal</em> partner</h2>
        <div class="section-body">
          <p>
            Meals on the Move provides high-quality, freshly prepared meals to care facilities
            across Ontario. We understand that the people in your care have specific nutritional
            needs — and that your staff's time is better spent on care than on cooking.
          </p>
          <p>
            Our kitchen operates with strict allergen protocols and produces every meal fresh each morning.
            We work closely with each facility to build menus that meet their population's dietary requirements,
            without compromising on taste.
          </p>
          <p>
            All client relationships begin with a conversation — reach out to our team to learn how
            we can support your facility.
          </p>
        </div>
      </div>
      <div class="about-stats">
        <div class="about-stat">
          <div class="about-stat-num">Same <em>day</em></div>
          <div class="about-stat-label">Fresh delivery, every order</div>
          <div class="about-stat-sub">Cooked each morning and delivered by mealtime — never reheated from frozen.</div>
        </div>
        <div class="about-stat">
          <div class="about-stat-num"><em>4</em> free</div>
          <div class="about-stat-label">Allergens excluded from every meal</div>
          <div class="about-stat-sub">Peanut, tree nut, sesame, and shellfish — strict protocols across every recipe, every day.</div>
        </div>
        <div class="about-stat">
          <div class="about-stat-num">Local <em>kitchen</em></div>
          <div class="about-stat-label">Toronto-based production</div>
          <div class="about-stat-sub">105 Vanderhoof Ave, Unit 8 — made in Ontario, delivered across Ontario.</div>
        </div>
      </div>
    </div>
  </div>
</section>

<!-- Sectors -->
<section class="sectors" id="sectors">
  <div class="container">
    <div class="sectors-header">
      <div class="section-eyebrow">Who we serve</div>
      <h2 class="section-title" style="font-size: clamp(28px, 3vw, 40px);">Built for the people who care for others</h2>
    </div>
    <div class="sectors-grid">

      <div class="sector-card">
        <div class="sector-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
            <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/>
            <circle cx="9" cy="7" r="4"/>
            <path d="M23 21v-2a4 4 0 0 0-3-3.87"/>
            <path d="M16 3.13a4 4 0 0 1 0 7.75"/>
          </svg>
        </div>
        <p class="sector-name">Childcare</p>
        <p class="sector-desc">
          We provide nutritious, age-appropriate meals and snacks for childcare centres and daycares.
          Our menus are designed with children's developmental nutritional needs in mind, and every
          meal leaves your kitchen staff free to focus on the children in their care.
        </p>
        <div class="sector-tags">
          <span class="sector-tag">Breakfast</span>
          <span class="sector-tag">Lunch</span>
          <span class="sector-tag">Snacks</span>
          <span class="sector-tag">Dietary accommodations</span>
        </div>
      </div>

      <div class="sector-card">
        <div class="sector-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
            <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
            <polyline points="9 22 9 12 15 12 15 22"/>
          </svg>
        </div>
        <p class="sector-name">Residential Care</p>
        <p class="sector-desc">
          For long-term care homes and assisted living facilities, we offer full-day meal programmes
          tailored to the dietary and textural requirements of older adults. Consistent quality,
          reliable delivery, and menus that residents look forward to.
        </p>
        <div class="sector-tags">
          <span class="sector-tag">Full-day programmes</span>
          <span class="sector-tag">Texture-modified</span>
          <span class="sector-tag">Soft diets</span>
          <span class="sector-tag">Allergen managed</span>
        </div>
      </div>

      <div class="sector-card">
        <div class="sector-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75">
            <path d="M22 12h-4l-3 9L9 3l-3 9H2"/>
          </svg>
        </div>
        <p class="sector-name">Healthcare</p>
        <p class="sector-desc">
          We support hospitals, clinics, and outpatient facilities with fresh meal options for patients
          and staff. Our kitchen is experienced in accommodating complex dietary profiles, and we work
          with your nutrition team to ensure meals align with care plans.
        </p>
        <div class="sector-tags">
          <span class="sector-tag">Patient meals</span>
          <span class="sector-tag">Staff catering</span>
          <span class="sector-tag">Therapeutic diets</span>
          <span class="sector-tag">Vegan & vegetarian</span>
        </div>
      </div>

    </div>
  </div>
</section>

<!-- More time for care -->
<section class="care">
  <div class="container">
    <div class="care-grid">
      <div>
        <div class="section-eyebrow">Why Meals on the Move</div>
        <h2 class="section-title">More time<br/><em>for care</em></h2>
        <div class="section-body">
          <p>
            Running a kitchen is expensive, time-consuming, and a distraction from what your
            organisation does best — caring for people. We take that burden off your hands entirely.
          </p>
        </div>
        <div class="care-points">
          <div class="care-point">
            <div class="care-point-dot"></div>
            <p class="care-point-text"><strong>No kitchen overhead.</strong> No staffing, no equipment, no procurement headaches. We handle it all.</p>
          </div>
          <div class="care-point">
            <div class="care-point-dot"></div>
            <p class="care-point-text"><strong>Consistent quality, every day.</strong> Same kitchen, same standards — your residents and children get a reliable mealtime experience.</p>
          </div>
          <div class="care-point">
            <div class="care-point-dot"></div>
            <p class="care-point-text"><strong>Flexible and responsive.</strong> Menus adapt to your population's needs. Dietary changes, seasonal updates, and special occasions are all accommodated.</p>
          </div>
          <div class="care-point">
            <div class="care-point-dot"></div>
            <p class="care-point-text"><strong>Allergen accountability.</strong> Every meal produced under strict allergen protocols. Full transparency on ingredients and preparation.</p>
          </div>
        </div>
      </div>
      <div class="care-visual">
        <div class="care-tile">
          <div class="care-tile-label">Our promise</div>
          <div class="care-tile-value">Fresh daily</div>
          <div class="care-tile-sub">Every meal cooked the morning of delivery — never reheated from frozen</div>
        </div>
        <div class="care-tile">
          <div class="care-tile-label">Dietary options</div>
          <div class="care-tile-value" style="font-size: 20px; padding-top: 4px;">Dairy free · Egg free<br/>Vegan · Vegetarian<br/>+ more</div>
        </div>
        <div class="care-tile">
          <div class="care-tile-label">Delivery</div>
          <div class="care-tile-value" style="font-size: 20px; padding-top: 4px;">Hot or chilled<br/>to your door</div>
        </div>
      </div>
    </div>
  </div>
</section>

<!-- Contact -->
<section class="contact" id="contact">
  <div class="container">
    <div class="contact-header">
      <div class="section-eyebrow">Get in touch</div>
      <h2 class="section-title" style="font-size: clamp(28px, 3vw, 40px);">Start the conversation</h2>
      <p style="font-size: 15px; color: var(--muted); margin-top: 12px; max-width: 480px; margin-left: auto; margin-right: auto; line-height: 1.6; font-weight: 300;">
        All client relationships begin with a conversation. Reach out to our team to learn how
        Meals on the Move can support your facility.
      </p>
    </div>
    <div class="contact-grid">
      <div class="contact-card">
        <div class="contact-role">Sales</div>
        <div class="contact-name">Brad McKay</div>
        <a href="tel:+16138631613" class="contact-detail">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07A19.5 19.5 0 0 1 4.69 12a19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 3.6 1.26h3a2 2 0 0 1 2 1.72c.127.96.361 1.903.7 2.81a2 2 0 0 1-.45 2.11L7.91 8.84a16 16 0 0 0 6.29 6.29l.96-.96a2 2 0 0 1 2.11-.45c.907.339 1.85.573 2.81.7A2 2 0 0 1 22 16.92z"/>
          </svg>
          613-863-1613
        </a>
        <a href="mailto:brad@mealsonthemove.ca" class="contact-detail">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/>
            <polyline points="22,6 12,13 2,6"/>
          </svg>
          brad@mealsonthemove.ca
        </a>
      </div>

      <div class="contact-card">
        <div class="contact-role">Sales</div>
        <div class="contact-name">Samantha King</div>
        <a href="tel:+16472998291" class="contact-detail">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07A19.5 19.5 0 0 1 4.69 12a19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 3.6 1.26h3a2 2 0 0 1 2 1.72c.127.96.361 1.903.7 2.81a2 2 0 0 1-.45 2.11L7.91 8.84a16 16 0 0 0 6.29 6.29l.96-.96a2 2 0 0 1 2.11-.45c.907.339 1.85.573 2.81.7A2 2 0 0 1 22 16.92z"/>
          </svg>
          647-299-8291
        </a>
        <a href="mailto:sam@mealsonthemove.ca" class="contact-detail">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/>
            <polyline points="22,6 12,13 2,6"/>
          </svg>
          sam@mealsonthemove.ca
        </a>
      </div>

      <div class="contact-card order-desk">
        <div class="contact-role">Existing clients</div>
        <div class="contact-name">Order desk</div>
        <a href="tel:18664562121" class="contact-detail">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07A19.5 19.5 0 0 1 4.69 12a19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 3.6 1.26h3a2 2 0 0 1 2 1.72c.127.96.361 1.903.7 2.81a2 2 0 0 1-.45 2.11L7.91 8.84a16 16 0 0 0 6.29 6.29l.96-.96a2 2 0 0 1 2.11-.45c.907.339 1.85.573 2.81.7A2 2 0 0 1 22 16.92z"/>
          </svg>
          1-866-456-2121
        </a>
      </div>

    </div>
  </div>
</section>

<!-- Footer -->
<footer>
  <div class="footer-left">
    <a href="/" class="footer-logo">
      <div class="footer-logo-mark">
        <svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg">
          <path d="M5 1v4a2 2 0 0 0 1.5 1.937V15h1V6.937A2 2 0 0 0 9 5V1H8v3.5a.5.5 0 0 1-1 0V1H6v3.5a.5.5 0 0 1-1 0V1H5zM11 1c-.552 0-1.5 1.343-1.5 3s.948 2.937 1.5 2.937V15h1V6.937C12.552 6.937 13.5 5.657 13.5 4S12.552 1 11 1z"/>
        </svg>
      </div>
      <span class="footer-logo-name">Meals on the Move</span>
    </a>
    <p class="footer-address">105 Vanderhoof Ave, Unit 8, Toronto ON M4G 2H7</p>
  </div>
  <p class="footer-copy">© Meals on the Move 2026</p>
</footer>
