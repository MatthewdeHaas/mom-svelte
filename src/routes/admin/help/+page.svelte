<script lang="ts">
  // Single scrollable help page. All content is static — no load data,
  // no form actions, no reactivity. The TOC on the left is a simple
  // list of in-page anchors (no scroll-spy): the admin opens this
  // page occasionally for reference, not continuously, and scroll-spy
  // adds listener machinery that would need maintenance for one user.
  //
  // Style lean on the existing --motm-* tokens defined in the admin
  // +layout.svelte. No new design primitives. Target reading width is
  // ~70ch so prose stays comfortable on a wide monitor.
</script>

<style>
  .wrap {
    display: grid;
    grid-template-columns: 180px 1fr;
    gap: 36px;
    max-width: 960px;
    margin: 0 auto;
    color: var(--motm-text);
  }
  @media (max-width: 768px) {
    .wrap { grid-template-columns: 1fr; gap: 20px; }
    .toc { position: static !important; }
  }

  .page-header h1 {
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.02em;
    margin: 0 0 4px;
  }
  .page-header p {
    font-size: 13.5px;
    color: var(--motm-text-muted);
    margin: 0 0 24px;
  }

  /* TOC — sticky sidebar inside the content area */
  .toc {
    position: sticky;
    top: 72px;
    align-self: start;
    font-size: 12.5px;
    line-height: 1.9;
  }
  .toc-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--motm-text-muted);
    margin: 0 0 6px;
  }
  .toc a {
    display: block;
    color: var(--motm-text-muted);
    text-decoration: none;
    padding: 2px 0;
    border-left: 2px solid transparent;
    padding-left: 8px;
    margin-left: -8px;
    transition: color 120ms, border-color 120ms;
  }
  .toc a:hover {
    color: var(--motm-text);
    border-left-color: var(--motm-pink);
  }

  /* Body */
  article { line-height: 1.6; font-size: 14px; }
  article h2 {
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
    margin: 32px 0 10px;
    padding-top: 8px;
    scroll-margin-top: 72px;
  }
  article h2:first-of-type { margin-top: 0; }
  article h3 {
    font-size: 14.5px;
    font-weight: 600;
    margin: 20px 0 6px;
    color: var(--motm-text);
  }
  article p { margin: 0 0 10px; }
  article ul, article ol { margin: 0 0 10px; padding-left: 20px; }
  article li { margin-bottom: 4px; }
  article code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12.5px;
    background: #F3F0EC;
    border: 1px solid var(--motm-border);
    border-radius: 4px;
    padding: 1px 5px;
  }
  article a { color: var(--motm-pink-dark); text-decoration: underline; text-decoration-thickness: 1px; text-underline-offset: 2px; }
  article a:hover { color: var(--motm-pink); }

  /* Callout box */
  .callout {
    border: 1px solid var(--motm-border);
    border-left: 3px solid var(--motm-pink);
    background: #FBF7F4;
    border-radius: 8px;
    padding: 10px 14px;
    margin: 12px 0;
    font-size: 13px;
  }
  .callout strong { font-weight: 600; }

  /* Spec table (matches the one on /admin/recipes/new) */
  table.spec {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
    margin: 10px 0 16px;
    background: #fff;
    border: 1px solid var(--motm-border);
    border-radius: 8px;
    overflow: hidden;
  }
  table.spec th, table.spec td {
    text-align: left;
    padding: 8px 12px;
    border-bottom: 1px solid var(--motm-border);
    vertical-align: top;
  }
  table.spec th {
    font-weight: 600;
    background: #F7F3EF;
    color: var(--motm-text);
    font-size: 12px;
    letter-spacing: 0.02em;
  }
  table.spec tr:last-child td { border-bottom: none; }

  .badge {
    display: inline-block;
    font-size: 11px;
    font-weight: 500;
    padding: 1px 7px;
    border-radius: 10px;
    border: 1px solid transparent;
  }
  .badge-req { background: var(--motm-pink-light); color: var(--motm-pink-dark); }
  .badge-opt { background: #F3F0EC; color: var(--motm-text-muted); border-color: var(--motm-border); }
</style>

<div class="page-header">
  <h1>Help</h1>
  <p>How the admin console models the business and how to drive it week-to-week.</p>
</div>

<div class="wrap">
  <!-- ─────────── TOC ─────────── -->
  <nav class="toc" aria-label="Help contents">
    <p class="toc-title">Contents</p>
    <a href="#overview">Overview</a>
    <a href="#companies">Companies</a>
    <a href="#persons">Persons</a>
    <a href="#allergens">Allergens</a>
    <a href="#recipes">Recipes</a>
    <a href="#ingredients">Ingredients</a>
    <a href="#orders">Orders</a>
    <a href="#importing">Importing</a>
    <a href="#dietary">Dietary carve-outs</a>
    <a href="#troubleshooting">Troubleshooting</a>
  </nav>

  <!-- ─────────── Body ─────────── -->
  <article>
    <h2 id="overview">Overview</h2>
    <p>
      The admin console models a weekly meal-production operation.
      <strong>Companies</strong> are your customers (retirement homes, senior-care centres, workplaces). Each company has <strong>Persons</strong> — the individuals eating the meals, each with their own allergy profile. The kitchen produces <strong>Recipes</strong> (named dishes) made of <strong>Ingredients</strong> (raw materials with allergen tags). Each week, you accept <strong>Orders</strong> — one row per company × recipe × serving-date × portion — usually by importing a spreadsheet.
    </p>
    <p>
      Day-to-day the console is where you add new customers, keep the recipe book current, and import the week's orders. Longer-tail work — adjusting allergen tags, correcting a person's dietary profile, pulling an invoice — all lives here too.
    </p>

    <h2 id="companies">Companies</h2>
    <p>
      A <strong>Company</strong> is the top-level customer. One company = one delivery destination = one line on the invoice.
    </p>
    <p>
      Create a company before you can import orders for it — the importer looks companies up by name, and if the name on the spreadsheet does not match an existing company you'll get a <code>Company "…" not found</code> error. Deactivating a company hides it from new imports but preserves its history.
    </p>
    <p>Example: <code>SLEC</code>, <code>Richmond Hill CHATS</code>, <code>Acme Corp</code>.</p>

    <h2 id="persons">Persons</h2>
    <p>
      A <strong>Person</strong> belongs to one Company. Persons hold the per-individual information that doesn't change week-to-week: name, dietary preferences, and — most importantly — <strong>Allergens</strong> they can't eat.
    </p>
    <p>
      Persons do not appear on the weekly orders spreadsheet. The admin manages the sheet of individuals (who eats what, when) in their own working document and translates it to a bulk production sheet before importing. The console's Persons page is where you record and update the allergen profile for each individual, so the kitchen can flag conflicts.
    </p>

    <h2 id="allergens">Allergens</h2>
    <p>
      <strong>Allergens</strong> are a shared catalog — one list, used from both ends:
    </p>
    <ul>
      <li>Attached to a <strong>Person</strong>, they describe what that individual <em>cannot eat</em>.</li>
      <li>Attached to an <strong>Ingredient</strong>, they describe what the ingredient <em>contains</em>.</li>
    </ul>
    <p>
      When a Recipe uses an Ingredient tagged with an allergen that one of its eaters is sensitive to, the console flags the conflict. Edit the catalog from <a href="/admin/allergens">Allergens</a>. Common entries: <code>Dairy</code>, <code>Gluten</code>, <code>Egg</code>, <code>Peanuts</code>, <code>Tree Nuts</code>, <code>Soy</code>, <code>Shellfish</code>, <code>Fish</code>.
    </p>
    <div class="callout">
      <strong>Tip.</strong> Deleting an allergen is only allowed when nothing references it. If the delete fails, the error tells you how many ingredients or persons still carry it — unlink those first.
    </div>

    <h2 id="recipes">Recipes</h2>
    <p>
      A <strong>Recipe</strong> is a named dish with an ingredient list. The dish's allergen profile is <em>derived</em> from its ingredients — you don't tag the recipe directly, you tag the ingredients and the recipe inherits.
    </p>
    <p>
      Create recipes one at a time from <a href="/admin/recipes">Recipes</a>, or import a batch from a spreadsheet at <a href="/admin/recipes/new">Recipes → New → Import from spreadsheet</a>. The import format is covered in <a href="#importing">Importing</a>.
    </p>

    <h2 id="ingredients">Ingredients</h2>
    <p>
      An <strong>Ingredient</strong> is the raw material of a recipe — <code>Milk</code>, <code>Olive Oil</code>, <code>Chicken Breast</code>. Each ingredient carries:
    </p>
    <ul>
      <li><strong>Allergens</strong> — one or more entries from the Allergen catalog.</li>
      <li><strong>Cost per unit</strong> (optional) — used by Financials for recipe margin.</li>
    </ul>
    <p>
      Keep ingredients granular. <code>Milk</code> and <code>Soy Milk</code> are different ingredients with different allergens; don't collapse them into one.
    </p>

    <h2 id="orders">Orders</h2>
    <p>
      An <strong>Order</strong> is one line of bulk production: for a given <strong>Company</strong>, make <strong>N portions</strong> of a given <strong>Recipe</strong>, portioned at a given size, delivered on a given <strong>Serving Date</strong>, with a given <strong>Storage</strong> treatment (chilled or frozen).
    </p>
    <p>
      Orders are not per-person. They are per-batch. The admin's working document breaks the week down by individual eaters; the production sheet aggregates those into bulk lines before import.
    </p>
    <p>Browse and edit orders at <a href="/admin/orders">Orders</a>.</p>

    <h2 id="importing">Importing</h2>

    <h3>Weekly orders spreadsheet</h3>
    <p>
      Upload at <a href="/admin/orders">Orders → Import</a>. The sheet is the admin's "WS 26 Master Prod Sheet" shape — one row per order, with columns for Customer, Recipe, Serving Date, Shipment Date, Meal, Order Amount, Portion Size, Day, Storage.
    </p>
    <p>
      <strong>Placeholder rows.</strong> If a row's <code>Order Amount</code> is literally <code>0</code>, it's treated as a placeholder (the admin's workflow pre-populates every active customer × recipe × day slot and leaves <code>0</code> on the ones not ordered this week). The row is skipped and a <code>zero_amount_row_skipped</code> warning is attached to the import summary. The row's position on the sheet is preserved — downstream errors still cite the original sheet row number.
    </p>

    <h3>Recipes spreadsheet</h3>
    <p>
      Upload at <a href="/admin/recipes/new">Recipes → New → Import from spreadsheet</a>. The sheet has one row per <em>ingredient</em>; rows that share a Recipe name form one recipe.
    </p>
    <table class="spec">
      <thead>
        <tr><th>Column</th><th>Required</th><th>Notes</th></tr>
      </thead>
      <tbody>
        <tr><td>Recipe</td><td><span class="badge badge-req">Required</span></td><td>Rows sharing a name form one recipe</td></tr>
        <tr><td>Ingredient</td><td><span class="badge badge-opt">Optional</span></td><td>Leave blank for a recipe with no ingredients</td></tr>
        <tr><td>Amount</td><td><span class="badge badge-opt">Optional</span></td><td>Positive decimal — required when Ingredient is set</td></tr>
        <tr><td>Unit</td><td><span class="badge badge-opt">Optional</span></td><td><code>kg</code>, <code>g</code>, <code>ml</code> — required when Ingredient is set</td></tr>
        <tr><td>Allergens</td><td><span class="badge badge-opt">Optional</span></td><td>Comma-separated: <code>Dairy, Gluten</code>. You can also leave the Allergens column out entirely.</td></tr>
      </tbody>
    </table>
    <p>
      The console ships an empty <a href="/admin/recipes/new/template">template download</a> on the import page. Use it as the base — the parser matches on header text, so renaming or reordering columns is safe but typos are not.
    </p>

    <h3>Warnings vs errors</h3>
    <p>
      Imports produce two kinds of signal:
    </p>
    <ul>
      <li><strong>Warnings</strong> — the row was handled, but something was unusual. Example: <code>zero_amount_row_skipped</code>. The import proceeds.</li>
      <li><strong>Errors</strong> — the row (and the whole import) was rejected. The console lists every error with its row number so you can fix the sheet and retry. One bad row aborts the batch.</li>
    </ul>

    <h2 id="dietary">Dietary carve-outs</h2>
    <p>
      Allergen matching runs from both ends:
    </p>
    <ol>
      <li>Each <strong>Person</strong> has a list of allergens they must avoid.</li>
      <li>Each <strong>Ingredient</strong> has a list of allergens it contains.</li>
      <li>A <strong>Recipe</strong> inherits the union of its ingredients' allergens.</li>
      <li>The console flags the intersection — "Person X at Company Y is allergic to <em>dairy</em>, and this week's Lentil Soup contains <em>dairy</em>".</li>
    </ol>
    <div class="callout">
      <strong>Imports and allergens.</strong> The weekly orders sheet is a bulk production doc — it does not identify eaters by name. Allergen conflicts surface against the <em>Company</em>'s roster of Persons on the kitchen display, not during the orders import itself.
    </div>

    <h2 id="troubleshooting">Troubleshooting</h2>

    <h3>"Amount must be greater than zero"</h3>
    <p>
      Something slipped past the placeholder filter. Open the sheet to the cited row — if the cell is literal <code>0</code>, the filter should have caught it; if the cell is a formula evaluating to <code>0</code> or a blank, replace it with a real positive number or delete the row.
    </p>

    <h3>"Amount contains a unit (e.g. <code>100g</code>); split the number into Amount and the unit (e.g. <code>g</code>) into Unit"</h3>
    <p>
      You're importing a recipes sheet where the Amount column has the unit baked into the same cell. Split <code>100g</code> into Amount <code>100</code> and Unit <code>g</code>. The 3-column legacy sheet shape (Recipe / Amount / Ingredient with units in the Amount cell) is not accepted directly — convert it to the 5-column format first.
    </p>

    <h3>"Company X not found"</h3>
    <p>
      The company name on the spreadsheet does not match any Company in the system. Case- and whitespace-insensitive, but spelling matters. Create the Company first, then re-import.
    </p>

    <h3>"Excel template missing columns"</h3>
    <p>
      The parser could not find a required header. The error names each missing column. Re-download the template from the import page and copy your data into the matching columns.
    </p>

    <h3>Import succeeded but the recipe looks wrong</h3>
    <p>
      Check the <strong>warnings</strong> on the import summary. Fuzzy name matches, auto-created ingredients, and skipped rows all surface as warnings — the import proceeded, but you may want to retry with a corrected sheet.
    </p>
  </article>
</div>
