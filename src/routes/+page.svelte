<script lang="ts">
  // No data needed — pure marketing page
</script>

<svelte:head>
  <title>Meals on the Move — Fresh meals delivered to care facilities</title>
  <meta name="description" content="Meals on the Move delivers fresh, allergen-conscious meals to childcare centres, residential care homes, and healthcare facilities across Ontario." />
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
  <link href="https://fonts.googleapis.com/css2?family=Playfair+Display:ital,wght@0,400;0,600;1,400&family=DM+Sans:opsz,wght@9..40,300;9..40,400;9..40,500;9..40,600&display=swap" rel="stylesheet" />
</svelte:head>

<!-- Nav -->
<nav class="sticky top-0 z-50 flex items-center justify-between h-[60px] px-6 md:px-16 bg-[#FAF8F4]/90 backdrop-blur-md border-b border-[#E8E0D6]">
  <a href="/" class="flex items-center gap-2.5 no-underline">
		<img src="/mom-logo.svg" alt="Meals on the Move" class="size-18" />
    <span class="text-[13.5px] font-semibold text-[#2C1F24] tracking-tight">Meals on the Move</span>
  </a>
  <a href="#contact" class="px-4 py-1.5 bg-[#2E1521] text-white text-[13px] font-medium rounded-lg hover:bg-[#4A2535] transition-colors no-underline">
    Get in touch
  </a>
</nav>

<!-- Hero -->
<section class="relative overflow-hidden bg-[#2E1521] px-6 md:px-16 py-24 md:py-36">
  <!-- Glow effects -->
  <div class="absolute -top-40 -right-40 w-[500px] h-[500px] rounded-full bg-[#C9718A]/10 blur-3xl pointer-events-none"></div>
  <div class="absolute -bottom-40 -left-20 w-[350px] h-[350px] rounded-full bg-[#C9718A]/6 blur-3xl pointer-events-none"></div>

  <div class="max-w-5xl mx-auto relative z-10">
    <div class="flex items-center gap-2 mb-7">
      <div class="w-6 h-[1.5px] bg-[#C9718A]"></div>
      <span class="text-[11px] font-semibold tracking-[0.1em] uppercase text-[#C9718A]">Ontario's care meal specialists</span>
    </div>

    <h1 class="font-['Playfair_Display'] text-5xl md:text-7xl font-semibold text-white leading-[1.1] tracking-tight max-w-2xl mb-3">
      Better food.<br/>
      <em class="text-[#C9718A] not-italic font-['Playfair_Display'] italic">More time for care.</em>
    </h1>

    <p class="text-lg md:text-xl text-white/50 font-light max-w-lg leading-relaxed mb-10">
      Fresh, allergen-conscious meals cooked daily and delivered directly to childcare centres,
      residential care homes, and healthcare facilities across Ontario.
    </p>

    <div class="flex flex-wrap items-center gap-4">
      <a href="#contact" class="px-7 py-3.5 bg-[#C9718A] hover:bg-[#9E4E66] text-white font-semibold rounded-xl transition-colors no-underline text-[14.5px]">
        Get in touch
      </a>
      <a href="#sectors" class="px-7 py-3.5 border border-white/20 hover:border-white/50 text-white/65 hover:text-white font-medium rounded-xl transition-colors no-underline text-[14.5px]">
        Who we serve
      </a>
    </div>

    <div class="mt-14 pt-8 border-t border-white/10 flex flex-wrap items-start gap-4">
      <span class="text-[11px] font-semibold tracking-[0.08em] uppercase text-white/35 pt-0.5 whitespace-nowrap">All meals are</span>
      <div class="flex flex-wrap gap-2">
        {#each ['Peanut free', 'Tree nut free', 'Sesame free', 'Shellfish free'] as tag}
          <span class="px-2.5 py-1 border border-white/15 rounded-full text-[12px] text-white/60 font-medium">{tag}</span>
        {/each}
      </div>
    </div>
  </div>
</section>

<!-- Pillars -->
<div class="bg-[#F0EBE3] border-y border-[#E8E0D6]">
  <div class="max-w-5xl mx-auto grid grid-cols-2 md:grid-cols-4">
    {#each [
      { icon: 'heart', label: 'Great tasting meals' },
      { icon: 'clock', label: 'Cooked fresh & delivered same day' },
      { icon: 'pin', label: 'Produced locally in Toronto' },
      { icon: 'temp', label: 'Available hot or chilled' },
    ] as pillar, i}
      <div class="px-6 py-7 {i < 3 ? 'border-r border-[#E8E0D6]' : ''} flex flex-col gap-3">
        <div class="w-9 h-9 rounded-[9px] bg-[#F5DCE4] flex items-center justify-center">
          {#if pillar.icon === 'heart'}
            <svg class="w-4 h-4 text-[#9E4E66]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z"/></svg>
          {:else if pillar.icon === 'clock'}
            <svg class="w-4 h-4 text-[#9E4E66]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
          {:else if pillar.icon === 'pin'}
            <svg class="w-4 h-4 text-[#9E4E66]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><path d="M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7z"/><circle cx="12" cy="9" r="2.5"/></svg>
          {:else}
            <svg class="w-4 h-4 text-[#9E4E66]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>
          {/if}
        </div>
        <p class="text-[14px] font-semibold text-[#2C1F24] leading-snug">{pillar.label}</p>
      </div>
    {/each}
  </div>
</div>

<!-- About -->
<section class="px-6 md:px-16 py-20 md:py-28 bg-[#FAF8F4]">
  <div class="max-w-5xl mx-auto grid grid-cols-1 md:grid-cols-2 gap-16 md:gap-20 items-start">
    <div>
      <div class="flex items-center gap-2 mb-4">
        <div class="w-5 h-[1.5px] bg-[#C9718A]"></div>
        <span class="text-[11.5px] font-semibold tracking-[0.1em] uppercase text-[#C9718A]">About us</span>
      </div>
      <h2 class="font-['Playfair_Display'] text-4xl md:text-5xl font-semibold text-[#2C1F24] leading-[1.15] tracking-tight mb-5">
        Ontario's trusted <em class="italic text-[#4A2535]">care meal</em> partner
      </h2>
      <div class="space-y-4 text-[15.5px] text-[#7A6670] leading-relaxed font-light">
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

    <!-- Pull stats -->
    <div class="border-l-2 border-[#F5DCE4] pl-10 flex flex-col divide-y divide-[#E8E0D6]">
      {#each [
        { num: 'Same', numEm: 'day', label: 'Fresh delivery, every order', sub: 'Cooked each morning and delivered by mealtime — never reheated from frozen.' },
        { num: '4', numEm: 'free', label: 'Allergens excluded from every meal', sub: 'Peanut, tree nut, sesame, and shellfish — strict protocols across every recipe, every day.' },
        { num: 'Local', numEm: 'kitchen', label: 'Toronto-based production', sub: '105 Vanderhoof Ave, Unit 8 — made in Ontario, delivered across Ontario.' },
      ] as stat}
        <div class="py-7">
          <div class="font-['Playfair_Display'] text-4xl md:text-5xl font-semibold text-[#2E1521] leading-none tracking-tight mb-1.5">
            {stat.num} <em class="italic text-[#C9718A]">{stat.numEm}</em>
          </div>
          <div class="text-[14px] font-semibold text-[#2C1F24] mb-1">{stat.label}</div>
          <div class="text-[13px] text-[#7A6670] leading-relaxed font-light">{stat.sub}</div>
        </div>
      {/each}
    </div>
  </div>
</section>

<!-- Sectors -->
<section id="sectors" class="px-6 md:px-16 py-20 md:py-28 bg-[#FAF8F4] border-t border-[#E8E0D6]">
  <div class="max-w-5xl mx-auto">
    <div class="text-center mb-14">
      <span class="text-[11.5px] font-semibold tracking-[0.1em] uppercase text-[#C9718A]">Who we serve</span>
      <h2 class="font-['Playfair_Display'] text-3xl md:text-4xl font-semibold text-[#2C1F24] leading-[1.15] tracking-tight mt-3">
        Built for the people who care for others
      </h2>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-3 gap-5">
      {#each [
        {
          icon: 'people',
          name: 'Childcare',
          desc: 'We provide nutritious, age-appropriate meals and snacks for childcare centres and daycares. Our menus are designed with children\'s developmental nutritional needs in mind, and every meal leaves your kitchen staff free to focus on the children in their care.',
          tags: ['Breakfast', 'Lunch', 'Snacks', 'Dietary accommodations'],
        },
        {
          icon: 'home',
          name: 'Residential Care',
          desc: 'For long-term care homes and assisted living facilities, we offer full-day meal programmes tailored to the dietary and textural requirements of older adults. Consistent quality, reliable delivery, and menus that residents look forward to.',
          tags: ['Full-day programmes', 'Texture-modified', 'Soft diets', 'Allergen managed'],
        },
        {
          icon: 'pulse',
          name: 'Healthcare',
          desc: 'We support hospitals, clinics, and outpatient facilities with fresh meal options for patients and staff. Our kitchen is experienced in accommodating complex dietary profiles, and we work with your nutrition team to ensure meals align with care plans.',
          tags: ['Patient meals', 'Staff catering', 'Therapeutic diets', 'Vegan & vegetarian'],
        },
      ] as sector}
        <div class="bg-white border border-[#E8E0D6] rounded-2xl p-8 flex flex-col gap-3.5 hover:border-[#C9718A] hover:shadow-[0_4px_20px_rgba(201,113,138,0.1)] transition-all">
          <div class="w-11 h-11 rounded-xl bg-[#F5DCE4] flex items-center justify-center">
            {#if sector.icon === 'people'}
              <svg class="w-5 h-5 text-[#9E4E66]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>
            {:else if sector.icon === 'home'}
              <svg class="w-5 h-5 text-[#9E4E66]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>
            {:else}
              <svg class="w-5 h-5 text-[#9E4E66]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>
            {/if}
          </div>
          <p class="font-['Playfair_Display'] text-xl font-semibold text-[#2C1F24] tracking-tight">{sector.name}</p>
          <p class="text-[14px] text-[#7A6670] leading-relaxed font-light flex-1">{sector.desc}</p>
          <div class="flex flex-wrap gap-1.5 mt-1">
            {#each sector.tags as tag}
              <span class="px-2.5 py-0.5 bg-[#F0EBE3] rounded-full text-[12px] font-medium text-[#7A6670]">{tag}</span>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  </div>
</section>

<!-- More time for care -->
<section class="px-6 md:px-16 py-20 md:py-28 bg-[#2E1521]">
  <div class="max-w-5xl mx-auto grid grid-cols-1 md:grid-cols-2 gap-16 items-start">
    <div>
      <div class="flex items-center gap-2 mb-4">
        <div class="w-5 h-[1.5px] bg-[#C9718A]"></div>
        <span class="text-[11.5px] font-semibold tracking-[0.1em] uppercase text-[#C9718A]">Why Meals on the Move</span>
      </div>
      <h2 class="font-['Playfair_Display'] text-4xl md:text-5xl font-semibold text-white leading-[1.15] tracking-tight mb-5">
        More time<br/><em class="italic text-[#C9718A]">for care</em>
      </h2>
      <p class="text-[15.5px] text-white/55 font-light leading-relaxed mb-8">
        Running a kitchen is expensive, time-consuming, and a distraction from what your
        organisation does best — caring for people. We take that burden off your hands entirely.
      </p>
      <div class="flex flex-col gap-5">
        {#each [
          { title: 'No kitchen overhead.', body: 'No staffing, no equipment, no procurement headaches. We handle it all.' },
          { title: 'Consistent quality, every day.', body: 'Same kitchen, same standards — your residents and children get a reliable mealtime experience.' },
          { title: 'Flexible and responsive.', body: 'Menus adapt to your population\'s needs. Dietary changes, seasonal updates, and special occasions are all accommodated.' },
          { title: 'Allergen accountability.', body: 'Every meal produced under strict allergen protocols. Full transparency on ingredients and preparation.' },
        ] as point}
          <div class="flex gap-3.5 items-start">
            <div class="w-2 h-2 rounded-full bg-[#C9718A] flex-shrink-0 mt-2"></div>
            <p class="text-[14.5px] text-white/70 font-light leading-relaxed">
              <strong class="text-white font-medium">{point.title}</strong> {point.body}
            </p>
          </div>
        {/each}
      </div>
    </div>

    <!-- Tiles -->
    <div class="grid grid-cols-2 gap-2.5">
      <div class="col-span-2 bg-[#C9718A]/15 border border-[#C9718A]/25 rounded-xl p-6">
        <div class="text-[11px] font-semibold tracking-[0.05em] uppercase text-white/35 mb-2">Our promise</div>
        <div class="font-['Playfair_Display'] text-3xl font-semibold text-white tracking-tight">Fresh daily</div>
        <div class="text-[12.5px] text-white/40 mt-1.5 leading-relaxed">Every meal cooked the morning of delivery — never reheated from frozen</div>
      </div>
      <div class="bg-white/6 border border-white/10 rounded-xl p-5">
        <div class="text-[11px] font-semibold tracking-[0.05em] uppercase text-white/35 mb-2">Dietary options</div>
        <div class="text-[13.5px] text-white/75 leading-relaxed">Dairy free · Egg free<br/>Vegan · Vegetarian<br/>+ more</div>
      </div>
      <div class="bg-white/6 border border-white/10 rounded-xl p-5">
        <div class="text-[11px] font-semibold tracking-[0.05em] uppercase text-white/35 mb-2">Delivery</div>
        <div class="text-[13.5px] text-white/75 leading-relaxed">Hot or chilled<br/>to your door</div>
      </div>
    </div>
  </div>
</section>

<!-- Contact -->
<section id="contact" class="px-6 md:px-16 py-20 md:py-28 bg-[#FAF8F4]">
  <div class="max-w-5xl mx-auto">
    <div class="text-center mb-14">
      <span class="text-[11.5px] font-semibold tracking-[0.1em] uppercase text-[#C9718A]">Get in touch</span>
      <h2 class="font-['Playfair_Display'] text-3xl md:text-4xl font-semibold text-[#2C1F24] leading-[1.15] tracking-tight mt-3 mb-3">
        Start the conversation
      </h2>
      <p class="text-[15px] text-[#7A6670] font-light leading-relaxed max-w-md mx-auto">
        All client relationships begin with a conversation. Reach out to our team to learn how
        Meals on the Move can support your facility.
      </p>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
      {#each [
        { role: 'Sales', name: 'Brad McKay', phone: '613-863-1613', phoneHref: 'tel:+16138631613', email: 'brad@mealsonthemove.ca' },
        { role: 'Sales', name: 'Samantha King', phone: '647-299-8291', phoneHref: 'tel:+16472998291', email: 'sam@mealsonthemove.ca' },
      ] as contact}
        <div class="bg-white border border-[#E8E0D6] rounded-xl p-7 flex flex-col gap-1.5 hover:border-[#C9718A] transition-colors">
          <div class="text-[11px] font-semibold tracking-[0.08em] uppercase text-[#C9718A] mb-1">{contact.role}</div>
          <div class="font-['Playfair_Display'] text-xl font-semibold text-[#2C1F24] mb-2">{contact.name}</div>
          <a href={contact.phoneHref} class="flex items-center gap-2 text-[13.5px] text-[#7A6670] no-underline hover:text-[#2C1F24] transition-colors">
            <svg class="w-3.5 h-3.5 opacity-50 flex-shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07A19.5 19.5 0 0 1 4.69 12a19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 3.6 1.26h3a2 2 0 0 1 2 1.72c.127.96.361 1.903.7 2.81a2 2 0 0 1-.45 2.11L7.91 8.84a16 16 0 0 0 6.29 6.29l.96-.96a2 2 0 0 1 2.11-.45c.907.339 1.85.573 2.81.7A2 2 0 0 1 22 16.92z"/></svg>
            {contact.phone}
          </a>
          <a href="mailto:{contact.email}" class="flex items-center gap-2 text-[13.5px] text-[#7A6670] no-underline hover:text-[#2C1F24] transition-colors">
            <svg class="w-3.5 h-3.5 opacity-50 flex-shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/><polyline points="22,6 12,13 2,6"/></svg>
            {contact.email}
          </a>
        </div>
      {/each}

      <div class="bg-[#2E1521] border border-transparent rounded-xl p-7 flex flex-col gap-1.5">
        <div class="text-[11px] font-semibold tracking-[0.08em] uppercase text-[#C9718A] mb-1">Existing clients</div>
        <div class="font-['Playfair_Display'] text-xl font-semibold text-white mb-2">Order desk</div>
        <a href="tel:18664562121" class="flex items-center gap-2 text-[13.5px] text-white/60 no-underline hover:text-white transition-colors">
          <svg class="w-3.5 h-3.5 opacity-50 flex-shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07A19.5 19.5 0 0 1 4.69 12a19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 3.6 1.26h3a2 2 0 0 1 2 1.72c.127.96.361 1.903.7 2.81a2 2 0 0 1-.45 2.11L7.91 8.84a16 16 0 0 0 6.29 6.29l.96-.96a2 2 0 0 1 2.11-.45c.907.339 1.85.573 2.81.7A2 2 0 0 1 22 16.92z"/></svg>
          1-866-456-2121
        </a>
      </div>
    </div>
  </div>
</section>

<!-- Footer -->
<footer class="bg-[#2E1521] border-t border-white/6 px-6 md:px-16 py-8 flex flex-wrap items-center justify-between gap-3">
  <div class="flex flex-col gap-1">
    <a href="/" class="flex items-center gap-2 no-underline">
			<img src="/mom-logo.svg" alt="" class="size-12" />
      <span class="text-[13px] font-semibold text-white/80">Meals on the Move</span>
    </a>
    <p class="text-[12px] text-white/30 ml-8">105 Vanderhoof Ave, Unit 8, Toronto ON M4G 2H7</p>
  </div>
  <p class="text-[12px] text-white/25">© Meals on the Move 2026</p>
</footer>
