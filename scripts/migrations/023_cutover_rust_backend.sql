-- 023_cutover_rust_backend.sql
--
-- Track 3 Step 6: flip every `use_rust_backend_*` flag to
-- `enabled_global = TRUE`. From this point forward the TS query
-- modules under `src/lib/server/queries/` are deleted — the only
-- remaining TS path is the local xlsx+pdf pipeline in
-- `src/lib/excel/export/*` and `src/lib/pdf/packing-slip.ts`, which
-- stays as a dead-but-reachable fall-through so a flag flip on
-- `use_rust_backend_exports` reverts to the known-good local
-- pipeline without a code deploy.
--
-- Rollback for query domains (`orders`, `recipes`, …): flip these
-- back to FALSE AND `git revert` the cutover commit — the dispatcher
-- wrappers in `_rust-dispatch.ts` have no local SQL fall-through
-- anymore. The TS modules have to exist for the SQL path to run.
--
-- Rollback for exports: flip `use_rust_backend_exports` to FALSE —
-- the local ExcelJS/pdfkit pipeline is still in-tree and runs when
-- the flag is off.

UPDATE feature_flag SET enabled_global = TRUE
 WHERE name LIKE 'use_rust_backend_%';
