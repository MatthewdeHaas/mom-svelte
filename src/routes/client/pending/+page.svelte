<style>
  .pending {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 60vh;
    text-align: center;
    padding: 32px 20px;
  }

  .icon {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: #FEF3C7;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 20px;
  }
  .icon svg { width: 24px; height: 24px; color: #92400E; }

  h1 {
    font-size: 20px;
    font-weight: 600;
    color: #2C1F24;
    letter-spacing: -0.02em;
    margin: 0 0 8px;
  }

  p {
    font-size: 14px;
    color: #7A6670;
    line-height: 1.6;
    max-width: 380px;
    margin: 0;
  }

  .contact {
    margin-top: 24px;
    font-size: 13px;
    color: #7A6670;
  }
  .contact a {
    color: #C9718A;
    text-decoration: none;
    font-weight: 500;
  }
  .contact a:hover { text-decoration: underline; }
</style>

<div class="pending">
  <div class="icon">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="12" cy="12" r="10"/>
      <path d="M12 6v6l4 2"/>
    </svg>
  </div>

  <h1>Access pending</h1>
  <p>
    Your account has been created but hasn't been linked to a company yet.
    Your account manager will get you set up shortly.
  </p>

  <p class="contact">
    Questions? Email
    <a href="mailto:sam@mealsonthemove.ca">sam@mealsonthemove.ca</a>
  </p>
</div>
