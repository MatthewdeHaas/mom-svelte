<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData, ActionData } from './$types';

  const { data, form }: { data: PageData; form: ActionData } = $props();

  const errors = $derived(
    form?.action === 'create' ? (form?.errors as Record<string, string> ?? {}) : {}
  );
  const values = $derived(
    form?.action === 'create' ? (form?.values as Record<string, string> ?? {}) : {}
  );

  // Local state for the amount input so user edits are preserved across re-renders
  let amountValue = $state(values?.amount ?? '');

  // Sync from form action result (e.g. after validation failure echoes the value back)
  $effect(() => {
    if (form?.action === 'create' && (form?.values as Record<string, string>)?.amount) {
      amountValue = (form.values as Record<string, string>).amount;
    }
  });

  function formatDate(d: string) {
    return new Date(d).toLocaleDateString('en-CA', {
      weekday: 'long', year: 'numeric', month: 'long', day: 'numeric',
    });
  }
</script>

<style>
  .back-link {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: 13px; color: #7A6670; text-decoration: none;
    margin-bottom: 20px; transition: color 120ms;
  }
  .back-link:hover { color: #2C1F24; }

  h1 { font-size: 22px; font-weight: 600; color: #2C1F24; letter-spacing: -0.02em; margin: 0 0 4px; }
  .sub { font-size: 14px; color: #7A6670; margin: 0 0 28px; }

  .card { background: #fff; border: 1px solid #EDE8E3; border-radius: 10px; max-width: 520px; }
  .card-header { padding: 18px 22px 14px; border-bottom: 1px solid #EDE8E3; }
  .card-title { font-size: 14px; font-weight: 600; color: #2C1F24; }
  .card-sub { font-size: 12.5px; color: #7A6670; margin-top: 2px; }
  .card-body { padding: 22px; display: flex; flex-direction: column; gap: 16px; }
  .card-footer {
    padding: 14px 22px; border-top: 1px solid #EDE8E3;
    display: flex; gap: 10px; justify-content: flex-end;
  }

  .summary-row {
    display: flex; justify-content: space-between; font-size: 13.5px;
    padding: 6px 0; border-bottom: 1px solid #EDE8E3;
  }
  .summary-row:last-child { border-bottom: none; }
  .summary-label { color: #7A6670; }
  .summary-value { font-weight: 500; color: #2C1F24; }

  .field { display: flex; flex-direction: column; gap: 5px; }
  label { font-size: 12.5px; font-weight: 600; color: #2C1F24; }
  input, textarea {
    padding: 8px 11px; border: 1px solid #EDE8E3; border-radius: 7px;
    font-size: 13.5px; font-family: inherit; color: #2C1F24; background: #fff;
    outline: none; transition: border-color 120ms;
  }
  input:focus, textarea:focus { border-color: #C9718A; }
  input.error { border-color: #E24B4A; }
  .field-error { font-size: 12px; color: #A32D2D; }
  .label-opt { font-weight: 400; color: #7A6670; margin-left: 4px; }

  .storage-group { display: flex; gap: 8px; flex-wrap: wrap; }
  .storage-option {
    display: flex; align-items: center; gap: 7px;
    padding: 7px 14px; border: 1px solid #EDE8E3; border-radius: 8px;
    cursor: pointer; font-size: 13px; color: #2C1F24;
    transition: border-color 120ms, background 120ms;
    user-select: none;
  }
  .storage-option:has(input:checked) { border-color: #C9718A; background: #FFF8FA; }
  .storage-option input[type="radio"] { accent-color: #C9718A; }

  .btn {
    padding: 8px 16px; border-radius: 8px; font-size: 13.5px; font-weight: 500;
    font-family: inherit; cursor: pointer; border: none; transition: background 120ms;
  }
  .btn-primary { background: #C9718A; color: #fff; }
  .btn-primary:hover { background: #b05e77; }
  .btn-ghost { background: transparent; color: #7A6670; border: 1px solid #EDE8E3; text-decoration: none; }
  .btn-ghost:hover { background: #FAF8F6; color: #2C1F24; }
</style>

<a href="/client/menu" class="back-link">
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
    <path d="M19 12H5M12 5l-7 7 7 7"/>
  </svg>
  Menu
</a>

<h1>Place order</h1>
<p class="sub">Review and confirm your order details.</p>

<div class="card">
  <div class="card-header">
    <p class="card-title">{data.recipeName}</p>
    <p class="card-sub">{data.mealCategory} · {formatDate(data.servingDate)}</p>
  </div>

  <form method="POST" action="?/create" use:enhance>
    <!-- Hidden fields carry the menu slot context -->
    <input type="hidden" name="menu_recipe_id" value={data.menuRecipeId} />
    <input type="hidden" name="recipe_id" value={data.recipeId} />
    <input type="hidden" name="serving_date" value={data.servingDate} />
    <input type="hidden" name="meal_category" value={data.mealCategory} />

    <div class="card-body">
      <div class="field">
        <label for="amount">Number of meals</label>
        <input
          id="amount"
          name="amount"
          type="number"
          min="1"
          step="1"
          bind:value={amountValue}
          class:error={errors?.amount || errors?._form}
          placeholder="e.g. 25"
          required
        />
        {#if errors?.amount || errors?._form}
          <span class="field-error">{errors?.amount ?? errors._form}</span>
        {/if}
      </div>

      <div class="field">
        <label>Storage preference</label>
        <div class="storage-group">
          <label class="storage-option">
            <input type="radio" name="storage" value="ambient"
              checked={!values?.storage || values?.storage === 'ambient'} />
            Ambient
          </label>
          <label class="storage-option">
            <input type="radio" name="storage" value="chilled"
              checked={values?.storage === 'chilled'} />
            Chilled
          </label>
          <label class="storage-option">
            <input type="radio" name="storage" value="frozen"
              checked={values?.storage === 'frozen'} />
            Frozen
          </label>
        </div>
      </div>

      <div class="field">
        <label for="client_note">
          Notes
          <span class="label-opt">(optional)</span>
        </label>
        <textarea
          id="client_note"
          name="client_note"
          rows="3"
          placeholder="Any special instructions or dietary notes…"
        >{values?.client_note ?? ''}</textarea>
      </div>
    </div>

    <div class="card-footer">
      <a href="/client/menu" class="btn btn-ghost">Cancel</a>
      <button type="submit" class="btn btn-primary">Confirm order</button>
    </div>
  </form>
</div>
