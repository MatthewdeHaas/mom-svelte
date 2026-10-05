//! Parity integration tests for `/api/allergens/*`.
//!
//! Mirrors `tests/integration/core/allergens.test.ts` 1:1 — same shapes,
//! same assertions, same edge cases — plus Rust-only client-scoping
//! tests the TS suite can't express (no HTTP boundary there) and
//! `get_ref_count` coverage (TS has none; the admin route uses it
//! inline).
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB — tests are embarrassingly parallel.

mod common;

use common::{
    allergens::{
        attach_allergen_to_ingredient, attach_allergen_to_person, seed_allergen, seed_company,
        seed_ingredient, seed_person,
    },
    seed_user, signed_headers, spawn_server, TestServer,
};
use db::queries::allergens::{AllergenRow, RefCount};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as ingredients.rs. Reproduced locally so
// each test file is self-contained (ingredients precedent).
// ---------------------------------------------------------------------------

async fn get_admin(server: &TestServer, user_id: &str, path: &str) -> reqwest::Response {
    let mut req = Client::new().get(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("GET")
}

async fn get_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
) -> reqwest::Response {
    let mut req = Client::new().get(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("GET client")
}

async fn post_admin(
    server: &TestServer,
    user_id: &str,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .post(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("POST")
}

async fn post_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .post(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("POST client")
}

async fn patch_admin(
    server: &TestServer,
    user_id: &str,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .patch(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("PATCH")
}

async fn patch_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .patch(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("PATCH client")
}

async fn delete_admin(server: &TestServer, user_id: &str, path: &str) -> reqwest::Response {
    let mut req = Client::new().delete(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("DELETE")
}

async fn delete_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
) -> reqwest::Response {
    let mut req = Client::new().delete(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("DELETE client")
}

async fn decode_ok<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> T {
    let status = res.status();
    let text = res.text().await.expect("body");
    assert!(status.is_success(), "non-2xx: status={status} body={text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("decode: {e}; body={text}"))
}

const BOGUS: &str = "00000000-0000-0000-0000-000000000000";

// ---------------------------------------------------------------------------
// GET /api/allergens — list
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_empty_when_none_exist(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AllergenRow> =
        decode_ok(get_admin(&server, &user.id, "/api/allergens").await).await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_all_inserted(pool: PgPool) {
    seed_allergen(&pool, "Peanut").await;
    seed_allergen(&pool, "Gluten").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AllergenRow> =
        decode_ok(get_admin(&server, &user.id, "/api/allergens").await).await;
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();

    assert!(names.contains(&"Peanut"));
    assert!(names.contains(&"Gluten"));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_orders_rows_by_name_ascending(pool: PgPool) {
    seed_allergen(&pool, "Zinc").await;
    seed_allergen(&pool, "Almond").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AllergenRow> =
        decode_ok(get_admin(&server, &user.id, "/api/allergens").await).await;
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();

    let a = names.iter().position(|n| *n == "Almond").expect("Almond");
    let z = names.iter().position(|n| *n == "Zinc").expect("Zinc");
    assert!(a < z);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_rows_have_expected_shape(pool: PgPool) {
    let id = seed_allergen(&pool, "Soy").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AllergenRow> =
        decode_ok(get_admin(&server, &user.id, "/api/allergens").await).await;
    let found = rows.iter().find(|r| r.id == id).expect("found");
    assert_eq!(found.name, "Soy");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/allergens/:id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/allergens/{BOGUS}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_happy_path(pool: PgPool) {
    let id = seed_allergen(&pool, "Shellfish").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: AllergenRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/allergens/{id}")).await).await;
    assert_eq!(row.id, id);
    assert_eq!(row.name, "Shellfish");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/allergens/by-name — case-insensitive
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_name_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, "/api/allergens/by-name?name=Ghost").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_name_case_insensitive(pool: PgPool) {
    let id = seed_allergen(&pool, "Peanut").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Query with a different case; LOWER(name) = LOWER($1) must match.
    let row: AllergenRow =
        decode_ok(get_admin(&server, &user.id, "/api/allergens/by-name?name=PEANUT").await).await;
    assert_eq!(row.id, id);
    assert_eq!(row.name, "Peanut");

    let row2: AllergenRow =
        decode_ok(get_admin(&server, &user.id, "/api/allergens/by-name?name=peanut").await).await;
    assert_eq!(row2.id, id);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/allergens — create
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_returns_inserted_row(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(&server, &user.id, "/api/allergens", &json!({ "name": "Wheat" })).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let row: AllergenRow = decode_ok(res).await;
    assert_eq!(row.name, "Wheat");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_trims_whitespace(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/allergens",
        &json!({ "name": "  Sesame  " }),
    )
    .await;
    let row: AllergenRow = decode_ok(res).await;
    assert_eq!(row.name, "Sesame");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_persists_so_appears_in_list(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin(&server, &user.id, "/api/allergens", &json!({ "name": "Mustard" })).await;

    let rows: Vec<AllergenRow> =
        decode_ok(get_admin(&server, &user.id, "/api/allergens").await).await;
    assert!(rows.iter().any(|r| r.name == "Mustard"));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_rejects_duplicate_name_with_409(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let first =
        post_admin(&server, &user.id, "/api/allergens", &json!({ "name": "Lupin" })).await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second =
        post_admin(&server, &user.id, "/api/allergens", &json!({ "name": "Lupin" })).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// PATCH /api/allergens/:id — rename
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn update_renames_and_returns_updated_row(pool: PgPool) {
    let id = seed_allergen(&pool, "OldName").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/allergens/{id}"),
        &json!({ "name": "NewName" }),
    )
    .await;
    let row: AllergenRow = decode_ok(res).await;
    assert_eq!(row.id, id);
    assert_eq!(row.name, "NewName");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/allergens/{BOGUS}"),
        &json!({ "name": "Ghost" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_trims_whitespace(pool: PgPool) {
    let id = seed_allergen(&pool, "Before").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/allergens/{id}"),
        &json!({ "name": "  After  " }),
    )
    .await;
    let row: AllergenRow = decode_ok(res).await;
    assert_eq!(row.name, "After");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_change_reflected_in_subsequent_read(pool: PgPool) {
    let id = seed_allergen(&pool, "Before").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    patch_admin(
        &server,
        &user.id,
        &format!("/api/allergens/{id}"),
        &json!({ "name": "After" }),
    )
    .await;

    let row: AllergenRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/allergens/{id}")).await).await;
    assert_eq!(row.name, "After");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_rejects_duplicate_name_with_409(pool: PgPool) {
    let a = seed_allergen(&pool, "AlphaDup").await;
    seed_allergen(&pool, "BetaDup").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Rename `AlphaDup` to the already-taken `BetaDup` → 409.
    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/allergens/{a}"),
        &json!({ "name": "BetaDup" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/allergens/:id/ref-count
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn ref_count_zero_for_unlinked_allergen(pool: PgPool) {
    let id = seed_allergen(&pool, "Lonely").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let counts: RefCount =
        decode_ok(get_admin(&server, &user.id, &format!("/api/allergens/{id}/ref-count")).await)
            .await;
    assert_eq!(counts.ingredients, 0);
    assert_eq!(counts.persons, 0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn ref_count_zero_for_missing_id_returns_200(pool: PgPool) {
    // No pre-existence check — missing id yields {0, 0}, mirroring TS semantics
    // (two independent subqueries ignore the parent row's existence).
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let counts: RefCount = decode_ok(
        get_admin(&server, &user.id, &format!("/api/allergens/{BOGUS}/ref-count")).await,
    )
    .await;
    assert_eq!(counts.ingredients, 0);
    assert_eq!(counts.persons, 0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn ref_count_counts_ingredient_links(pool: PgPool) {
    let a = seed_allergen(&pool, "LinkedI").await;
    let i1 = seed_ingredient(&pool, "I1").await;
    let i2 = seed_ingredient(&pool, "I2").await;
    attach_allergen_to_ingredient(&pool, i1, a).await;
    attach_allergen_to_ingredient(&pool, i2, a).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let counts: RefCount =
        decode_ok(get_admin(&server, &user.id, &format!("/api/allergens/{a}/ref-count")).await)
            .await;
    assert_eq!(counts.ingredients, 2);
    assert_eq!(counts.persons, 0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn ref_count_counts_person_links(pool: PgPool) {
    let a = seed_allergen(&pool, "LinkedP").await;
    let c = seed_company(&pool, "RefCountCo").await;
    let p1 = seed_person(&pool, c, "Person One").await;
    let p2 = seed_person(&pool, c, "Person Two").await;
    attach_allergen_to_person(&pool, p1, a).await;
    attach_allergen_to_person(&pool, p2, a).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let counts: RefCount =
        decode_ok(get_admin(&server, &user.id, &format!("/api/allergens/{a}/ref-count")).await)
            .await;
    assert_eq!(counts.ingredients, 0);
    assert_eq!(counts.persons, 2);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn ref_count_counts_both_sides(pool: PgPool) {
    let a = seed_allergen(&pool, "LinkedBoth").await;
    let i = seed_ingredient(&pool, "SharedIng").await;
    let c = seed_company(&pool, "SharedCo").await;
    let p = seed_person(&pool, c, "Shared Person").await;
    attach_allergen_to_ingredient(&pool, i, a).await;
    attach_allergen_to_person(&pool, p, a).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let counts: RefCount =
        decode_ok(get_admin(&server, &user.id, &format!("/api/allergens/{a}/ref-count")).await)
            .await;
    assert_eq!(counts.ingredients, 1);
    assert_eq!(counts.persons, 1);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// DELETE /api/allergens/:id — hard delete
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn delete_removes_the_row(pool: PgPool) {
    let id = seed_allergen(&pool, "Celery").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = delete_admin(&server, &user.id, &format!("/api/allergens/{id}")).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Follow-up get must 404.
    let get_res = get_admin(&server, &user.id, &format!("/api/allergens/{id}")).await;
    assert_eq!(get_res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn delete_404_for_missing_id(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = delete_admin(&server, &user.id, &format!("/api/allergens/{BOGUS}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn delete_does_not_affect_other_allergens(pool: PgPool) {
    let a = seed_allergen(&pool, "Sulphites").await;
    let b = seed_allergen(&pool, "Molluscs").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    delete_admin(&server, &user.id, &format!("/api/allergens/{a}")).await;

    let remaining: AllergenRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/allergens/{b}")).await).await;
    assert_eq!(remaining.id, b);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn delete_cascades_ingredient_allergen_rows(pool: PgPool) {
    // Join tables use ON DELETE CASCADE. Deleting the allergen must
    // silently drop its join rows — the admin UI's pre-check via
    // `get_ref_count` is the UX guard; the DB itself allows the delete.
    //
    // Verify via the HTTP surface only (no direct DB handle after
    // spawn_server consumes the pool): seed a link, see it in ref-count,
    // delete the allergen, confirm 404 on the allergen read and the
    // ingredient still exists.
    let a = seed_allergen(&pool, "CascadeI").await;
    let i = seed_ingredient(&pool, "CascadeIng").await;
    attach_allergen_to_ingredient(&pool, i, a).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Pre-delete: ref-count sees the one ingredient link.
    let before: RefCount =
        decode_ok(get_admin(&server, &user.id, &format!("/api/allergens/{a}/ref-count")).await)
            .await;
    assert_eq!(before.ingredients, 1);

    // Delete the allergen; cascades silently.
    let res = delete_admin(&server, &user.id, &format!("/api/allergens/{a}")).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Allergen itself is gone.
    let after = get_admin(&server, &user.id, &format!("/api/allergens/{a}")).await;
    assert_eq!(after.status(), StatusCode::NOT_FOUND);

    // Ingredient is unaffected. (Admin-only read; we don't need the
    // link set — just that the ingredient is still there.)
    let ing = get_admin(&server, &user.id, &format!("/api/ingredients/{i}")).await;
    assert_eq!(ing.status(), StatusCode::OK);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Client scoping — Rust-only. Allergens are admin-only everywhere; a
// client gets 403 uniformly across every endpoint. No "lie about
// existence" since no `/api/allergens` client route exists today.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_list_allergens(pool: PgPool) {
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, own, "/api/allergens").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_get_allergen_by_id(pool: PgPool) {
    let id = seed_allergen(&pool, "AdminOnlyA").await;
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, own, &format!("/api/allergens/{id}")).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_get_allergen_by_name(pool: PgPool) {
    seed_allergen(&pool, "AdminOnlyB").await;
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        own,
        "/api/allergens/by-name?name=AdminOnlyB",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_get_ref_count(pool: PgPool) {
    let id = seed_allergen(&pool, "AdminOnlyC").await;
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        own,
        &format!("/api/allergens/{id}/ref-count"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_create_allergen(pool: PgPool) {
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        own,
        "/api/allergens",
        &json!({ "name": "ClientTry" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_update_allergen(pool: PgPool) {
    let id = seed_allergen(&pool, "AdminOnlyD").await;
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_client(
        &server,
        &user.id,
        own,
        &format!("/api/allergens/{id}"),
        &json!({ "name": "Blocked" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_delete_allergen(pool: PgPool) {
    let id = seed_allergen(&pool, "AdminOnlyE").await;
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = delete_client(&server, &user.id, own, &format!("/api/allergens/{id}")).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}
