<script lang="ts">
  let { data } = $props();
</script>

<style>
  h1 { font-size: 22px; font-weight: 600; color: #2C1F24; letter-spacing: -0.02em; margin: 0 0 4px; }
  .sub { font-size: 14px; color: #7A6670; margin: 0 0 24px; }

  table { width: 100%; border-collapse: collapse; background: #fff; border: 1px solid #EDE8E3; border-radius: 12px; overflow: hidden; }
  th { font-size: 11.5px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase; color: #B8AEB3; text-align: left; padding: 12px 16px; border-bottom: 1px solid #EDE8E3; background: #FAF8F6; }
  td { font-size: 13.5px; color: #2C1F24; padding: 12px 16px; border-bottom: 1px solid #EDE8E3; vertical-align: top; }
  tr:last-child td { border-bottom: none; }
  tr:hover td { background: #FBF9F7; }

  .allergen-list { display: flex; flex-wrap: wrap; gap: 4px; }
  .allergen-tag {
    padding: 2px 8px;
    background: #FEF3C7;
    border: 1px solid #FDE68A;
    border-radius: 20px;
    font-size: 11.5px;
    font-weight: 500;
    color: #92400E;
  }
  .none { color: #B8AEB3; font-size: 13px; }

  .empty { font-size: 14px; color: #B8AEB3; padding: 32px 0; text-align: center; }
</style>

<h1>People</h1>
<p class="sub">People enrolled in your meal programme and their dietary requirements.</p>

{#if data.persons.length === 0}
  <p class="empty">No people added yet. Contact your account manager to update this list.</p>
{:else}
  <table>
    <thead>
      <tr>
        <th>Name</th>
        <th>Allergens</th>
      </tr>
    </thead>
    <tbody>
      {#each data.persons as person}
        <tr>
          <td>{person.name}</td>
          <td>
            {#if person.allergen.length > 0}
              <div class="allergen-list">
                {#each person.allergen as a}
                  <span class="allergen-tag">{a.name}</span>
                {/each}
              </div>
            {:else}
              <span class="none">None recorded</span>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
