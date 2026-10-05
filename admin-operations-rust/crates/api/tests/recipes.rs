//! Parity integration tests for `/api/recipes/*`.
//!
//! Mirrors `tests/integration/core/recipes.test.ts` 1:1 — same shapes,
//! same assertions, same edge cases. Plus Rust-only client-scoping
//! tests the TS suite can't express (no HTTP boundary there).
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB — tests are embarrassingly parallel.

mod common;

use common::{
    recipes::{
        attach_allergen_to_ingredient, attach_ingredient_to_recipe, seed_allergen,
        seed_ingredient, seed_recipe, seed_recipe_inactive,
    },
    seed_user, signed_headers, spawn_server, TestServer,
};
use db::queries::recipes::{RecipeIngredientRow, RecipeRow, RecipeWithIngredientsRow};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as orders.rs. Reproduced locally so each
// test file is self-contained (orders.rs does the same).
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
// GET /api/recipes — list
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_excludes_inactive_by_default(pool: PgPool) {
    let active = seed_recipe(&pool, "r-active").await;
    let inactive = seed_recipe_inactive(&pool, "r-inactive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeRow> =
        decode_ok(get_admin(&server, &user.id, "/api/recipes").await).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&active));
    assert!(!ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_includes_inactive_when_flag_set(pool: PgPool) {
    let inactive = seed_recipe_inactive(&pool, "r-inactive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeRow> = decode_ok(
        get_admin(&server, &user.id, "/api/recipes?include_inactive=true").await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_orders_rows_by_name_ascending(pool: PgPool) {
    seed_recipe(&pool, "Zucchini Soup").await;
    seed_recipe(&pool, "Apple Salad").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeRow> =
        decode_ok(get_admin(&server, &user.id, "/api/recipes").await).await;
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();

    let apple = names.iter().position(|n| *n == "Apple Salad").expect("Apple");
    let zucchini = names
        .iter()
        .position(|n| *n == "Zucchini Soup")
        .expect("Zucchini");
    assert!(apple < zucchini);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_rows_have_expected_shape(pool: PgPool) {
    let id = seed_recipe(&pool, "Shape Recipe").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeRow> =
        decode_ok(get_admin(&server, &user.id, "/api/recipes").await).await;
    let found = rows.iter().find(|r| r.id == id).expect("found");

    assert!(found.active);
    // RecipeRow carries a DateTime<Utc> for created_at; decoding
    // succeeded, so the shape is sound.
    let _ = found.created_at;

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/recipes/:id — get_by_id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/recipes/{BOGUS}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_404_for_inactive_without_flag(pool: PgPool) {
    let id = seed_recipe_inactive(&pool, "inactive-r").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/recipes/{id}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_inactive_when_flag_set(pool: PgPool) {
    let id = seed_recipe_inactive(&pool, "inactive-r").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: RecipeRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/recipes/{id}?include_inactive=true"),
        )
        .await,
    )
    .await;

    assert_eq!(row.id, id);
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_full_row(pool: PgPool) {
    let id = seed_recipe(&pool, "FullRow").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: RecipeRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/recipes/{id}")).await).await;

    assert_eq!(row.id, id);
    assert_eq!(row.name, "FullRow");
    assert!(row.active);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/recipes/by-name — get_by_name
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn by_name_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/recipes/by-name?name=Definitely+Not+Real",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn by_name_returns_404_for_inactive_without_flag(pool: PgPool) {
    seed_recipe_inactive(&pool, "InactiveName").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/recipes/by-name?name=InactiveName",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn by_name_returns_inactive_when_flag_set(pool: PgPool) {
    let id = seed_recipe_inactive(&pool, "InactiveName").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: RecipeRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/recipes/by-name?name=InactiveName&include_inactive=true",
        )
        .await,
    )
    .await;

    assert_eq!(row.id, id);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn by_name_is_case_insensitive(pool: PgPool) {
    let id = seed_recipe(&pool, "Case Soup").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: RecipeRow = decode_ok(
        get_admin(&server, &user.id, "/api/recipes/by-name?name=case+soup").await,
    )
    .await;

    assert_eq!(row.id, id);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/recipes/:id/ingredients — get_ingredients
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_ingredients_returns_empty_when_none(pool: PgPool) {
    let rid = seed_recipe(&pool, "Empty-R").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;

    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_ingredients_returns_correct_fields(pool: PgPool) {
    let rid = seed_recipe(&pool, "WithIng").await;
    let iid = seed_ingredient(&pool, "Flour").await;
    attach_ingredient_to_recipe(&pool, rid, iid, 150.0, "g").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ingredient_id, iid);
    assert_eq!(rows[0].name, "Flour");
    assert_eq!(rows[0].unit, "g");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_ingredients_amount_is_number(pool: PgPool) {
    let rid = seed_recipe(&pool, "NumAmount").await;
    let iid = seed_ingredient(&pool, "Milk").await;
    attach_ingredient_to_recipe(&pool, rid, iid, 42.5, "ml").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;

    assert!((rows[0].amount - 42.5).abs() < 1e-9);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_ingredients_orders_by_name(pool: PgPool) {
    let rid = seed_recipe(&pool, "Ordered").await;
    let z = seed_ingredient(&pool, "Zucchini").await;
    let a = seed_ingredient(&pool, "Aubergine").await;
    attach_ingredient_to_recipe(&pool, rid, z, 10.0, "g").await;
    attach_ingredient_to_recipe(&pool, rid, a, 20.0, "g").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    let aub = names.iter().position(|n| *n == "Aubergine").unwrap();
    let zuc = names.iter().position(|n| *n == "Zucchini").unwrap();
    assert!(aub < zuc);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_ingredients_includes_allergens(pool: PgPool) {
    let rid = seed_recipe(&pool, "Allergic").await;
    let iid = seed_ingredient(&pool, "Peanut").await;
    let aid = seed_allergen(&pool, "Nuts").await;
    attach_allergen_to_ingredient(&pool, iid, aid).await;
    attach_ingredient_to_recipe(&pool, rid, iid, 10.0, "g").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;

    assert!(rows[0].allergen.iter().any(|a| a.id == aid));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_ingredients_empty_allergens_when_none(pool: PgPool) {
    let rid = seed_recipe(&pool, "NoAllergens").await;
    let iid = seed_ingredient(&pool, "Water").await;
    attach_ingredient_to_recipe(&pool, rid, iid, 10.0, "g").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;

    assert!(rows[0].allergen.is_empty());

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/recipes/:id/with-ingredients — get_with_ingredients
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn with_ingredients_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{BOGUS}/with-ingredients"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_ingredients_404_for_inactive_without_flag(pool: PgPool) {
    let rid = seed_recipe_inactive(&pool, "InactiveWithIng").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/with-ingredients"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_ingredients_returns_empty_ingredients_array(pool: PgPool) {
    let rid = seed_recipe(&pool, "WithEmpty").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: RecipeWithIngredientsRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/recipes/{rid}/with-ingredients"),
        )
        .await,
    )
    .await;

    assert!(row.ingredients.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_ingredients_returns_populated(pool: PgPool) {
    let rid = seed_recipe(&pool, "Populated").await;
    let iid = seed_ingredient(&pool, "Chicken").await;
    attach_ingredient_to_recipe(&pool, rid, iid, 100.0, "g").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: RecipeWithIngredientsRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/recipes/{rid}/with-ingredients"),
        )
        .await,
    )
    .await;

    assert!(row.ingredients.iter().any(|i| i.ingredient_id == iid));

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/recipes/with-ingredients — list_with_ingredients
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_ingredients_excludes_inactive(pool: PgPool) {
    let inactive = seed_recipe_inactive(&pool, "Inactive-WI").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeWithIngredientsRow> = decode_ok(
        get_admin(&server, &user.id, "/api/recipes/with-ingredients").await,
    )
    .await;

    assert!(!rows.iter().any(|r| r.id == inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_ingredients_returns_arrays(pool: PgPool) {
    seed_recipe(&pool, "ArrayCheck").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeWithIngredientsRow> = decode_ok(
        get_admin(&server, &user.id, "/api/recipes/with-ingredients").await,
    )
    .await;

    // Vec is always an array — compile-time check. But also assert it
    // decoded without panic above.
    assert!(!rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_ingredients_no_cross_contamination(pool: PgPool) {
    let r1 = seed_recipe(&pool, "R1-X").await;
    let r2 = seed_recipe(&pool, "R2-X").await;
    let i1 = seed_ingredient(&pool, "Ing1-X").await;
    let i2 = seed_ingredient(&pool, "Ing2-X").await;
    attach_ingredient_to_recipe(&pool, r1, i1, 10.0, "g").await;
    attach_ingredient_to_recipe(&pool, r2, i2, 20.0, "g").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeWithIngredientsRow> = decode_ok(
        get_admin(&server, &user.id, "/api/recipes/with-ingredients").await,
    )
    .await;

    let row1 = rows.iter().find(|r| r.id == r1).expect("r1");
    let row2 = rows.iter().find(|r| r.id == r2).expect("r2");

    assert!(row1.ingredients.iter().any(|i| i.ingredient_id == i1));
    assert!(row2.ingredients.iter().any(|i| i.ingredient_id == i2));
    assert!(!row1.ingredients.iter().any(|i| i.ingredient_id == i2));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_ingredients_unpacks_allergens(pool: PgPool) {
    let rid = seed_recipe(&pool, "AllergenR").await;
    let iid = seed_ingredient(&pool, "AllergenI").await;
    let aid = seed_allergen(&pool, "AllergenA").await;
    attach_allergen_to_ingredient(&pool, iid, aid).await;
    attach_ingredient_to_recipe(&pool, rid, iid, 10.0, "g").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeWithIngredientsRow> = decode_ok(
        get_admin(&server, &user.id, "/api/recipes/with-ingredients").await,
    )
    .await;

    let row = rows.iter().find(|r| r.id == rid).expect("row");
    let ing = row
        .ingredients
        .iter()
        .find(|i| i.ingredient_id == iid)
        .expect("ing");
    assert!(ing.allergen.iter().any(|a| a.id == aid));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_ingredients_amount_is_number(pool: PgPool) {
    let rid = seed_recipe(&pool, "NumAmount2").await;
    let iid = seed_ingredient(&pool, "NumIng").await;
    attach_ingredient_to_recipe(&pool, rid, iid, 99.9, "ml").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeWithIngredientsRow> = decode_ok(
        get_admin(&server, &user.id, "/api/recipes/with-ingredients").await,
    )
    .await;

    let row = rows.iter().find(|r| r.id == rid).expect("row");
    assert!((row.ingredients[0].amount - 99.9).abs() < 1e-9);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/recipes — create
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_returns_inserted_row(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/recipes",
        &json!({ "name": "NewRecipe" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let row: RecipeRow = decode_ok(res).await;

    assert_eq!(row.name, "NewRecipe");
    assert!(row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_trims_whitespace(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/recipes",
        &json!({ "name": "  Trimmed Recipe  " }),
    )
    .await;
    let row: RecipeRow = decode_ok(res).await;

    assert_eq!(row.name, "Trimmed Recipe");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_respects_explicit_inactive(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/recipes",
        &json!({ "name": "InactiveFromStart", "active": false }),
    )
    .await;
    let row: RecipeRow = decode_ok(res).await;

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
        "/api/recipes",
        &json!({ "name": "Dup" }),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = post_admin(
        &server,
        &user.id,
        "/api/recipes",
        &json!({ "name": "Dup" }),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CONFLICT);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// PATCH /api/recipes/:id — update
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn update_name_only_leaves_active_unchanged(pool: PgPool) {
    let id = seed_recipe(&pool, "OrigName").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{id}"),
        &json!({ "name": "NewName" }),
    )
    .await;
    let row: RecipeRow = decode_ok(res).await;

    assert_eq!(row.name, "NewName");
    assert!(row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_active_only_leaves_name_unchanged(pool: PgPool) {
    let id = seed_recipe(&pool, "OrigActive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{id}"),
        &json!({ "active": false }),
    )
    .await;
    let row: RecipeRow = decode_ok(res).await;

    assert!(!row.active);
    assert_eq!(row.name, "OrigActive");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_trims_whitespace_from_name(pool: PgPool) {
    let id = seed_recipe(&pool, "Trimmy").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{id}"),
        &json!({ "name": "  Trimmed  " }),
    )
    .await;
    let row: RecipeRow = decode_ok(res).await;

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
        &format!("/api/recipes/{BOGUS}"),
        &json!({ "name": "Ghost" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/recipes/:id/deactivate
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_sets_active_false(pool: PgPool) {
    let id = seed_recipe(&pool, "ToDeactivate").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/recipes/{id}/deactivate")).await;
    let row: RecipeRow = decode_ok(res).await;

    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivated_is_excluded_from_list(pool: PgPool) {
    let id = seed_recipe(&pool, "ToDeactivate2").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin_empty(&server, &user.id, &format!("/api/recipes/{id}/deactivate")).await;

    let rows: Vec<RecipeRow> =
        decode_ok(get_admin(&server, &user.id, "/api/recipes").await).await;
    assert!(!rows.iter().any(|r| r.id == id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivated_is_retrievable_with_include_inactive(pool: PgPool) {
    let id = seed_recipe(&pool, "ToDeactivate3").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin_empty(&server, &user.id, &format!("/api/recipes/{id}/deactivate")).await;

    let row: RecipeRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/recipes/{id}?include_inactive=true"),
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

    let res =
        post_admin_empty(&server, &user.id, &format!("/api/recipes/{BOGUS}/deactivate")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST / DELETE /api/recipes/:id/ingredients/:iid — add/remove
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn add_ingredient_associates(pool: PgPool) {
    let rid = seed_recipe(&pool, "AddIng").await;
    let iid = seed_ingredient(&pool, "ToAdd").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients/{iid}"),
        &json!({ "amount": 200.0, "unit": "g" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let ings: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;
    assert!(ings.iter().any(|i| i.ingredient_id == iid));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn add_ingredient_upserts_on_conflict(pool: PgPool) {
    let rid = seed_recipe(&pool, "Upsert").await;
    let iid = seed_ingredient(&pool, "Dupe").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients/{iid}"),
        &json!({ "amount": 100.0, "unit": "g" }),
    )
    .await;
    post_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients/{iid}"),
        &json!({ "amount": 250.0, "unit": "ml" }),
    )
    .await;

    let ings: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;

    assert_eq!(ings.len(), 1);
    assert!((ings[0].amount - 250.0).abs() < 1e-9);
    assert_eq!(ings[0].unit, "ml");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn remove_ingredient_only_removes_target(pool: PgPool) {
    let rid = seed_recipe(&pool, "Remove").await;
    let i1 = seed_ingredient(&pool, "Keep").await;
    let i2 = seed_ingredient(&pool, "Drop").await;
    attach_ingredient_to_recipe(&pool, rid, i1, 10.0, "g").await;
    attach_ingredient_to_recipe(&pool, rid, i2, 20.0, "g").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = delete_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients/{i2}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let ings: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;
    let ids: Vec<Uuid> = ings.iter().map(|i| i.ingredient_id).collect();
    assert!(ids.contains(&i1));
    assert!(!ids.contains(&i2));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn remove_ingredient_no_throw_when_missing(pool: PgPool) {
    let rid = seed_recipe(&pool, "RemoveMissing").await;
    let iid = seed_ingredient(&pool, "Never").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = delete_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients/{iid}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// PUT /api/recipes/:id/ingredients — set_ingredients
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn set_ingredients_applies_set(pool: PgPool) {
    let rid = seed_recipe(&pool, "SetIng").await;
    let i1 = seed_ingredient(&pool, "SI1").await;
    let i2 = seed_ingredient(&pool, "SI2").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = put_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients"),
        &json!({
            "ingredient": [
                { "ingredient_id": i1, "amount": 100.0, "unit": "g" },
                { "ingredient_id": i2, "amount": 200.0, "unit": "ml" },
            ]
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let ings: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;
    let ids: Vec<Uuid> = ings.iter().map(|i| i.ingredient_id).collect();

    assert!(ids.contains(&i1));
    assert!(ids.contains(&i2));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_ingredients_replaces_prior(pool: PgPool) {
    let rid = seed_recipe(&pool, "ReplacePrior").await;
    let old = seed_ingredient(&pool, "Old").await;
    let next = seed_ingredient(&pool, "Next").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    put_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients"),
        &json!({ "ingredient": [
            { "ingredient_id": old, "amount": 10.0, "unit": "g" }
        ]}),
    )
    .await;
    put_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients"),
        &json!({ "ingredient": [
            { "ingredient_id": next, "amount": 20.0, "unit": "g" }
        ]}),
    )
    .await;

    let ings: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;
    let ids: Vec<Uuid> = ings.iter().map(|i| i.ingredient_id).collect();

    assert!(!ids.contains(&old));
    assert!(ids.contains(&next));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_ingredients_empty_clears(pool: PgPool) {
    let rid = seed_recipe(&pool, "ClearAll").await;
    let iid = seed_ingredient(&pool, "Lonely").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    put_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients"),
        &json!({ "ingredient": [{ "ingredient_id": iid, "amount": 5.0, "unit": "g" }]}),
    )
    .await;
    put_admin(
        &server,
        &user.id,
        &format!("/api/recipes/{rid}/ingredients"),
        &json!({ "ingredient": [] }),
    )
    .await;

    let ings: Vec<RecipeIngredientRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{rid}/ingredients")).await,
    )
    .await;

    assert!(ings.is_empty());

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/recipes/:id/active — is_active
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn is_active_true_for_active(pool: PgPool) {
    let id = seed_recipe(&pool, "IsActive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body: Value =
        decode_ok(get_admin(&server, &user.id, &format!("/api/recipes/{id}/active")).await).await;
    assert_eq!(body["active"], json!(true));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn is_active_false_for_inactive(pool: PgPool) {
    let id = seed_recipe_inactive(&pool, "NotActive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body: Value =
        decode_ok(get_admin(&server, &user.id, &format!("/api/recipes/{id}/active")).await).await;
    assert_eq!(body["active"], json!(false));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn is_active_false_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body: Value = decode_ok(
        get_admin(&server, &user.id, &format!("/api/recipes/{BOGUS}/active")).await,
    )
    .await;
    assert_eq!(body["active"], json!(false));

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Client scoping — Rust-only. TS can't express these (no HTTP boundary).
// Recipes are global master data: clients read freely, writes return 403.
// ---------------------------------------------------------------------------

/// A valid UUID for a company that doesn't exist in the DB — the auth
/// layer only checks that the string parses; no FK on the auth payload.
const CLIENT_COMPANY: &str = "11111111-2222-3333-4444-555555555555";

fn client_company_id() -> Uuid {
    Uuid::parse_str(CLIENT_COMPANY).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_can_read_list(pool: PgPool) {
    seed_recipe(&pool, "ClientReadList").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, client_company_id(), "/api/recipes").await;
    assert_eq!(res.status(), StatusCode::OK);
    let _: Vec<RecipeRow> = decode_ok(res).await;

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_can_get_by_id(pool: PgPool) {
    let id = seed_recipe(&pool, "ClientGet").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        client_company_id(),
        &format!("/api/recipes/{id}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_create_recipe(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        client_company_id(),
        "/api/recipes",
        &json!({ "name": "ClientTryCreate" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_update_recipe(pool: PgPool) {
    let id = seed_recipe(&pool, "ClientCannotUpdate").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_client(
        &server,
        &user.id,
        client_company_id(),
        &format!("/api/recipes/{id}"),
        &json!({ "name": "Blocked" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_deactivate(pool: PgPool) {
    let id = seed_recipe(&pool, "ClientCannotDeactivate").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client_empty(
        &server,
        &user.id,
        client_company_id(),
        &format!("/api/recipes/{id}/deactivate"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}
