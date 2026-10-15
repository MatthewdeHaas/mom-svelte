 Why don't the e2e tests require a database connection? I thought that was the point of an e2e test. Also, is there a sense in splitting up the unit tests from integration tests so certain tests
  can run on their own, or is it better to just keep it as it is since enough of the intersting ones need a database connection? Also, would the insta snapshot test crates benefit this application
  at all? Also, I don't want to have to compile the ap every time in the e2e part and I want both jobs to share an envionrment, so that should be owrked on. I like the speed boost I got from the
  cached rust crates in the test job. That same speed boost should apply to the e2e steps as well. Also, is there a way to not have to wait 25 mins for a code change to the delpoy step? I know the
  test suite is green I don't need to run the CI checks right now.

---

## Answers (2026-10-14)

**Q1 — E2E tests and the DB.** They do use the DB; the misread was mine
earlier for not surfacing it. Every spec routes Playwright → SvelteKit
SSR → Rust API → Postgres:

- `tests/e2e/global-setup.ts::makePool` constructs `pg.Pool` from
  `POSTGRES_*` env.
- `tests/e2e/fixtures.ts` constructs `new Pool(...)` and runs raw SQL to
  seed users, companies, orders, recipes, ingredients, allergens, menus
  — all under `__e2e_test_*` names so cleanup is a single prefix match.
- CI (`ci.yaml`) and local Playwright (`playwright.config.ts`) both
  provision Postgres 18 as a service / docker container before specs
  run.
- No stubs, no mocks, no fake DB layer.

**Q2 — Split unit from integration.** Worth doing; PR #2 does it.
`crates/{excel,pdf,domain,flags-tui}` have ~66 pure `#[test]` and zero
`sqlx::test`. New `rust-unit` CI job runs those with no Postgres and no
migrations.

**Q3 — insta snapshot crate.** Yes, specifically for the determinism
tests. Current failure mode on byte drift is "left: [big byte array],
right: [big byte array]" — unreadable. insta turns that into a
reviewable `.snap` diff. PR #2 adds it to `excel`/`pdf` determinism
tests and the `/api/exports/*` digest checks.

**Q4 — Don't recompile Rust in E2E; share env across jobs.** PR #2
promotes `nix build .#api` to a dedicated `build-api` job that uploads
the binary as a GHA artifact; `e2e` downloads it rather than building.
Removes ~4-5 min cold from E2E's critical path.

**Q5 — 25 min wait for a deploy-step change.** PR #1 already added
`paths-ignore` on `push`/`pull_request` for
`.github/workflows/deploy.yaml`, `docs/**`, and `**.md` — a
deploy-only or docs-only change now skips CI entirely.
