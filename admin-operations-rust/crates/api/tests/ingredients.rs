//! Parity integration tests for `/api/ingredients/*`.
//!
//! Mirrors `tests/integration/core/ingredients.test.ts` 1:1 — same shapes,
//! same assertions, same edge cases. Plus Rust-only client-scoping
//! tests the TS suite can't express (no HTTP boundary there).
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB — tests are embarrassingly parallel.

mod common;

use common::{
    ingredients::{
        attach_allergen, seed_allergen, seed_ingredient, seed_ingredient_inactive,
        seed_ingredient_with_cost,
    },
    seed_user, signed_headers, spawn_server, TestServer,
};
use db::queries::ingredients::{IngredientRow, IngredientWithAllergensRow};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as companies.rs / recipes.rs. Reproduced
// locally so each test file is self-contained.
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

async fn post_admin_empty(server: &TestServer, user_id: &str, path: &str) -> reqwest::Response {
    let mut req = Client::new().post(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("POST empty")
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

async fn post_client_empty(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
) -> reqwest::Response {
    let mut req = Client::new().post(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("POST client empty")
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

async fn put_admin(
    server: &TestServer,
    user_id: &str,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .put(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("PUT")
}

async fn delete_admin(server: &TestServer, user_id: &str, path: &str) -> reqwest::Response {
    let mut req = Client::new().delete(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("DELETE")
}

async fn decode_ok<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> T {
    let status = res.status();
    let text = res.text().await.expect("body");
    assert!(status.is_success(), "non-2xx: status={status} body={text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("decode: {e}; body={text}"))
}

const BOGUS: &str = "00000000-0000-0000-0000-000000000000";

// ---------------------------------------------------------------------------
// GET /api/ingredients — list (flat)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_excludes_inactive_by_default(pool: PgPool) {
    let active = seed_ingredient(&pool, "i-active").await;
    let inactive = seed_ingredient_inactive(&pool, "i-inactive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<IngredientRow> =
        decode_ok(get_admin(&server, &user.id, "/api/ingredients").await).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&active));
    assert!(!ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_includes_inactive_when_flag_set(pool: PgPool) {
    let inactive = seed_ingredient_inactive(&pool, "i-inactive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<IngredientRow> = decode_ok(
        get_admin(&server, &user.id, "/api/ingredients?include_inactive=true").await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_orders_rows_by_name_ascending(pool: PgPool) {
    seed_ingredient(&pool, "Zeta").await;
    seed_ingredient(&pool, "Alpha").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<IngredientRow> =
        decode_ok(get_admin(&server, &user.id, "/api/ingredients").await).await;
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();

    let a = names.iter().position(|n| *n == "Alpha").expect("Alpha");
    let z = names.iter().position(|n| *n == "Zeta").expect("Zeta");
    assert!(a < z);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_rows_have_expected_shape(pool: PgPool) {
    let id = seed_ingredient_with_cost(&pool, "Shape", 1.25).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<IngredientRow> =
        decode_ok(get_admin(&server, &user.id, "/api/ingredients").await).await;
    let found = rows.iter().find(|r| r.id == id).expect("found");

    assert_eq!(found.name, "Shape");
    assert!(found.active);
    assert_eq!(found.cost_per_unit, Some(1.25));

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/ingredients/names
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_names_returns_ordered_name_strings(pool: PgPool) {
    seed_ingredient(&pool, "Zebra").await;
    seed_ingredient(&pool, "Apple").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let names: Vec<String> =
        decode_ok(get_admin(&server, &user.id, "/api/ingredients/names").await).await;
    assert!(names.contains(&"Apple".to_string()));
    assert!(names.contains(&"Zebra".to_string()));

    let a = names.iter().position(|n| n == "Apple").unwrap();
    let z = names.iter().position(|n| n == "Zebra").unwrap();
    assert!(a < z);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/ingredients/:id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/ingredients/{BOGUS}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_hides_inactive_by_default(pool: PgPool) {
    let id = seed_ingredient_inactive(&pool, "hidden").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/ingredients/{id}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_inactive_with_include_flag(pool: PgPool) {
    let id = seed_ingredient_inactive(&pool, "shown").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: IngredientRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{id}?include_inactive=true"),
        )
        .await,
    )
    .await;
    assert_eq!(row.id, id);
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_happy_path(pool: PgPool) {
    let id = seed_ingredient_with_cost(&pool, "HappyIngredient", 2.5).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: IngredientRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/ingredients/{id}")).await).await;
    assert_eq!(row.name, "HappyIngredient");
    assert_eq!(row.cost_per_unit, Some(2.5));

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/ingredients/:id/with-allergens
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_with_allergens_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{BOGUS}/with-allergens"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_with_allergens_empty_when_no_allergens(pool: PgPool) {
    let id = seed_ingredient(&pool, "Plain").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: IngredientWithAllergensRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{id}/with-allergens"),
        )
        .await,
    )
    .await;
    assert_eq!(row.id, id);
    assert!(row.allergen.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_with_allergens_happy_path(pool: PgPool) {
    let ing = seed_ingredient(&pool, "Peanut Butter").await;
    let a1 = seed_allergen(&pool, "nuts").await;
    attach_allergen(&pool, ing, a1).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: IngredientWithAllergensRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{ing}/with-allergens"),
        )
        .await,
    )
    .await;
    assert_eq!(row.allergen.len(), 1);
    assert_eq!(row.allergen[0].id, a1);
    assert_eq!(row.allergen[0].name, "nuts");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_with_allergens_orders_allergens_by_name(pool: PgPool) {
    let ing = seed_ingredient(&pool, "MixedIng").await;
    let z = seed_allergen(&pool, "zinc").await;
    let a = seed_allergen(&pool, "aspirin").await;
    attach_allergen(&pool, ing, z).await;
    attach_allergen(&pool, ing, a).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: IngredientWithAllergensRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{ing}/with-allergens"),
        )
        .await,
    )
    .await;
    let names: Vec<&str> = row.allergen.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["aspirin", "zinc"]);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/ingredients/with-allergens — flat list, nested allergens.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_allergens_empty_arrays_when_no_links(pool: PgPool) {
    let i1 = seed_ingredient(&pool, "SoloOne").await;
    let i2 = seed_ingredient(&pool, "SoloTwo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<IngredientWithAllergensRow> =
        decode_ok(get_admin(&server, &user.id, "/api/ingredients/with-allergens").await).await;
    let r1 = rows.iter().find(|r| r.id == i1).expect("i1");
    let r2 = rows.iter().find(|r| r.id == i2).expect("i2");
    assert!(r1.allergen.is_empty());
    assert!(r2.allergen.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_allergens_attaches_per_row(pool: PgPool) {
    let ing_a = seed_ingredient(&pool, "A-Ing").await;
    let ing_b = seed_ingredient(&pool, "B-Ing").await;
    let al1 = seed_allergen(&pool, "soy").await;
    let al2 = seed_allergen(&pool, "egg").await;
    attach_allergen(&pool, ing_a, al1).await;
    attach_allergen(&pool, ing_b, al2).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<IngredientWithAllergensRow> =
        decode_ok(get_admin(&server, &user.id, "/api/ingredients/with-allergens").await).await;
    let ra = rows.iter().find(|r| r.id == ing_a).expect("A");
    let rb = rows.iter().find(|r| r.id == ing_b).expect("B");

    assert_eq!(ra.allergen.len(), 1);
    assert_eq!(ra.allergen[0].name, "soy");
    assert_eq!(rb.allergen.len(), 1);
    assert_eq!(rb.allergen[0].name, "egg");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_allergens_excludes_inactive_by_default(pool: PgPool) {
    let active = seed_ingredient(&pool, "Live").await;
    let inactive = seed_ingredient_inactive(&pool, "Dead").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<IngredientWithAllergensRow> =
        decode_ok(get_admin(&server, &user.id, "/api/ingredients/with-allergens").await).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    assert!(ids.contains(&active));
    assert!(!ids.contains(&inactive));

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/ingredients — create
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_returns_inserted_row(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/ingredients",
        &json!({ "name": "NewIng" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let row: IngredientRow = decode_ok(res).await;

    assert_eq!(row.name, "NewIng");
    assert!(row.active);
    assert!(row.cost_per_unit.is_none());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_trims_whitespace(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/ingredients",
        &json!({ "name": "  Trimmed Ing  " }),
    )
    .await;
    let row: IngredientRow = decode_ok(res).await;
    assert_eq!(row.name, "Trimmed Ing");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_respects_explicit_inactive(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/ingredients",
        &json!({ "name": "InactiveFromStart", "active": false }),
    )
    .await;
    let row: IngredientRow = decode_ok(res).await;
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_rejects_duplicate_name_with_409(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let first = post_admin(
        &server,
        &user.id,
        "/api/ingredients",
        &json!({ "name": "DupIng" }),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = post_admin(
        &server,
        &user.id,
        "/api/ingredients",
        &json!({ "name": "DupIng" }),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CONFLICT);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// PATCH /api/ingredients/:id — update
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn update_name_only_leaves_other_fields_unchanged(pool: PgPool) {
    let id = seed_ingredient_with_cost(&pool, "BeforeName", 3.33).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{id}"),
        &json!({ "name": "AfterName" }),
    )
    .await;
    let row: IngredientRow = decode_ok(res).await;
    assert_eq!(row.name, "AfterName");
    assert_eq!(row.cost_per_unit, Some(3.33));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_active_only(pool: PgPool) {
    let id = seed_ingredient(&pool, "ToggleActive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{id}"),
        &json!({ "active": false }),
    )
    .await;
    let row: IngredientRow = decode_ok(res).await;
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_trims_whitespace_from_name(pool: PgPool) {
    let id = seed_ingredient(&pool, "Trimmy").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{id}"),
        &json!({ "name": "  Trimmed  " }),
    )
    .await;
    let row: IngredientRow = decode_ok(res).await;
    assert_eq!(row.name, "Trimmed");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{BOGUS}"),
        &json!({ "name": "Ghost" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_cost_per_unit_three_state(pool: PgPool) {
    // The three-state semantics: present → set; `null` → clear; absent → leave.
    let id = seed_ingredient_with_cost(&pool, "ThreeState", 5.00).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Set to a new value.
    let r1: IngredientRow = decode_ok(
        patch_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{id}"),
            &json!({ "cost_per_unit": 7.5 }),
        )
        .await,
    )
    .await;
    assert_eq!(r1.cost_per_unit, Some(7.5));

    // Clear to null.
    let r2: IngredientRow = decode_ok(
        patch_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{id}"),
            &json!({ "cost_per_unit": null }),
        )
        .await,
    )
    .await;
    assert!(r2.cost_per_unit.is_none());

    // Set then absent: value must be preserved from previous set.
    patch_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{id}"),
        &json!({ "cost_per_unit": 2.0 }),
    )
    .await;
    let r3: IngredientRow = decode_ok(
        patch_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{id}"),
            &json!({ "name": "NewName" }),
        )
        .await,
    )
    .await;
    assert_eq!(r3.cost_per_unit, Some(2.0));
    assert_eq!(r3.name, "NewName");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/ingredients/:id/deactivate
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_sets_active_false(pool: PgPool) {
    let id = seed_ingredient(&pool, "ToDeactivate").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/ingredients/{id}/deactivate"),
    )
    .await;
    let row: IngredientRow = decode_ok(res).await;
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivated_is_excluded_from_list(pool: PgPool) {
    let id = seed_ingredient(&pool, "ToDeactivate2").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin_empty(
        &server,
        &user.id,
        &format!("/api/ingredients/{id}/deactivate"),
    )
    .await;

    let rows: Vec<IngredientRow> =
        decode_ok(get_admin(&server, &user.id, "/api/ingredients").await).await;
    assert!(!rows.iter().any(|r| r.id == id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivated_is_retrievable_with_include_inactive(pool: PgPool) {
    let id = seed_ingredient(&pool, "ToDeactivate3").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin_empty(
        &server,
        &user.id,
        &format!("/api/ingredients/{id}/deactivate"),
    )
    .await;

    let row: IngredientRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{id}?include_inactive=true"),
        )
        .await,
    )
    .await;
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/ingredients/{BOGUS}/deactivate"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_does_not_affect_other_ingredients(pool: PgPool) {
    let a = seed_ingredient(&pool, "KeepA").await;
    let b = seed_ingredient(&pool, "DropB").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin_empty(
        &server,
        &user.id,
        &format!("/api/ingredients/{b}/deactivate"),
    )
    .await;

    let row: IngredientRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/ingredients/{a}")).await).await;
    assert!(row.active);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Allergen associations — add / remove / set
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn add_allergen_associates_and_is_idempotent(pool: PgPool) {
    let ing = seed_ingredient(&pool, "WithAllergen").await;
    let al = seed_allergen(&pool, "gluten").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let r1 = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/ingredients/{ing}/allergens/{al}"),
    )
    .await;
    assert_eq!(r1.status(), StatusCode::NO_CONTENT);

    // Second call must be a no-op (ON CONFLICT DO NOTHING), still 204.
    let r2 = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/ingredients/{ing}/allergens/{al}"),
    )
    .await;
    assert_eq!(r2.status(), StatusCode::NO_CONTENT);

    // Verify via with-allergens read.
    let row: IngredientWithAllergensRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{ing}/with-allergens"),
        )
        .await,
    )
    .await;
    assert_eq!(row.allergen.len(), 1);
    assert_eq!(row.allergen[0].id, al);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn remove_allergen_detaches_and_is_idempotent(pool: PgPool) {
    let ing = seed_ingredient(&pool, "Removal").await;
    let al = seed_allergen(&pool, "soy").await;
    attach_allergen(&pool, ing, al).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let r1 = delete_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{ing}/allergens/{al}"),
    )
    .await;
    assert_eq!(r1.status(), StatusCode::NO_CONTENT);

    // Repeat is a no-op.
    let r2 = delete_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{ing}/allergens/{al}"),
    )
    .await;
    assert_eq!(r2.status(), StatusCode::NO_CONTENT);

    let row: IngredientWithAllergensRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{ing}/with-allergens"),
        )
        .await,
    )
    .await;
    assert!(row.allergen.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_allergens_full_replace(pool: PgPool) {
    let ing = seed_ingredient(&pool, "SetMe").await;
    let a = seed_allergen(&pool, "aspirin-set").await;
    let b = seed_allergen(&pool, "butter-set").await;
    let c = seed_allergen(&pool, "carrot-set").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // First: {a, b}.
    let r1 = put_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{ing}/allergens"),
        &json!({ "allergen_ids": [a, b] }),
    )
    .await;
    assert_eq!(r1.status(), StatusCode::NO_CONTENT);

    let row1: IngredientWithAllergensRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{ing}/with-allergens"),
        )
        .await,
    )
    .await;
    let ids1: Vec<Uuid> = row1.allergen.iter().map(|a| a.id).collect();
    assert_eq!(ids1.len(), 2);
    assert!(ids1.contains(&a));
    assert!(ids1.contains(&b));

    // Second: {c} — must fully replace, not merge.
    put_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{ing}/allergens"),
        &json!({ "allergen_ids": [c] }),
    )
    .await;

    let row2: IngredientWithAllergensRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{ing}/with-allergens"),
        )
        .await,
    )
    .await;
    let ids2: Vec<Uuid> = row2.allergen.iter().map(|a| a.id).collect();
    assert_eq!(ids2, vec![c]);

    // Third: [] — clear completely.
    put_admin(
        &server,
        &user.id,
        &format!("/api/ingredients/{ing}/allergens"),
        &json!({ "allergen_ids": [] }),
    )
    .await;

    let row3: IngredientWithAllergensRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/ingredients/{ing}/with-allergens"),
        )
        .await,
    )
    .await;
    assert!(row3.allergen.is_empty());

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Client scoping — Rust-only. Ingredients are admin-only everywhere; a
// client gets 403 uniformly across reads and writes. No "lie about
// existence" since no client has legitimate access.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_list_ingredients(pool: PgPool) {
    let own = Uuid::new_v4();
    // Need a company row to satisfy FK on later queries; the client
    // header carries a UUID string — a bogus one is sufficient to prove
    // the 403 path (admin check runs before any DB touch).
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, own, "/api/ingredients").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_create_ingredient(pool: PgPool) {
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        own,
        "/api/ingredients",
        &json!({ "name": "ClientTry" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_update_ingredient(pool: PgPool) {
    let id = seed_ingredient(&pool, "AdminOnly").await;
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_client(
        &server,
        &user.id,
        own,
        &format!("/api/ingredients/{id}"),
        &json!({ "name": "Blocked" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_add_allergen(pool: PgPool) {
    let ing = seed_ingredient(&pool, "AdminOnly2").await;
    let al = seed_allergen(&pool, "nuts-clt").await;
    let own = Uuid::new_v4();
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client_empty(
        &server,
        &user.id,
        own,
        &format!("/api/ingredients/{ing}/allergens/{al}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}
