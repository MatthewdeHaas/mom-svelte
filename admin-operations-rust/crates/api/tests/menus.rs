//! Parity integration tests for `/api/menus/*`.
//!
//! Mirrors `tests/integration/core/menus.test.ts` 1:1 — same shapes,
//! same assertions, same edge cases — plus Rust-only client-scoping
//! tests the TS suite can't express (no HTTP boundary there).
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB — tests are embarrassingly parallel.

mod common;

use chrono::NaiveDate;
use common::{
    menus::{
        seed_menu, seed_menu_dates, seed_menu_inactive, seed_recipe, seed_slot, BASE_END,
        BASE_START,
    },
    seed_user, signed_headers, spawn_server, TestServer,
};
use db::queries::menus::{MenuRecipeRow, MenuRecipeWithNameRow, MenuRow, MenuWithRecipesRow};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as persons.rs. Reproduced locally so each
// test file is self-contained (precedent).
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

async fn put_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .put(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("PUT client")
}

async fn decode_ok<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> T {
    let status = res.status();
    let text = res.text().await.expect("body");
    assert!(status.is_success(), "non-2xx: status={status} body={text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("decode: {e}; body={text}"))
}

const BOGUS: &str = "00000000-0000-0000-0000-000000000000";

// ---------------------------------------------------------------------------
// GET /api/menus
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_empty_when_no_menus(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuRow> = decode_ok(get_admin(&server, &user.id, "/api/menus").await).await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_excludes_inactive_by_default(pool: PgPool) {
    let active = seed_menu(&pool, "Active Menu").await;
    let inactive = seed_menu_inactive(&pool, "Inactive Menu").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuRow> = decode_ok(get_admin(&server, &user.id, "/api/menus").await).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&active));
    assert!(!ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_includes_inactive_when_flag_set(pool: PgPool) {
    let inactive = seed_menu_inactive(&pool, "Hidden Menu").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuRow> =
        decode_ok(get_admin(&server, &user.id, "/api/menus?include_inactive=true").await).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    assert!(ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_orders_by_start_date_desc(pool: PgPool) {
    seed_menu_dates(
        &pool,
        "Early",
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
        true,
    )
    .await;
    seed_menu_dates(
        &pool,
        "Late",
        NaiveDate::from_ymd_opt(2025, 12, 1).unwrap(),
        NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
        true,
    )
    .await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuRow> = decode_ok(get_admin(&server, &user.id, "/api/menus").await).await;
    assert!(rows.len() >= 2);
    assert!(rows[0].start_date >= rows[1].start_date);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_expected_shape(pool: PgPool) {
    let id = seed_menu(&pool, "Shape Menu").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuRow> = decode_ok(get_admin(&server, &user.id, "/api/menus").await).await;
    let found = rows.iter().find(|r| r.id == id).expect("row present");
    assert!(found.is_active);
    assert_eq!(found.start_date, BASE_START);
    assert_eq!(found.end_date, BASE_END);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/menus/:id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/menus/{BOGUS}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_hides_inactive_by_default(pool: PgPool) {
    let id = seed_menu_inactive(&pool, "Hidden").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/menus/{id}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_inactive_with_include_flag(pool: PgPool) {
    let id = seed_menu_inactive(&pool, "Hidden").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/menus/{id}?include_inactive=true"),
        )
        .await,
    )
    .await;
    assert!(!row.is_active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_correct_menu(pool: PgPool) {
    let id = seed_menu(&pool, "Specific Menu").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{id}")).await).await;
    assert_eq!(row.id, id);
    assert_eq!(row.name, "Specific Menu");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/menus/:id/recipes
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_menu_recipes_empty_for_menu_without_slots(pool: PgPool) {
    let m = seed_menu(&pool, "No Slots").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_menu_recipes_denormalises_name_and_active(pool: PgPool) {
    let m = seed_menu(&pool, "Has Slot").await;
    let r = seed_recipe(&pool, "Pancakes").await;
    seed_slot(&pool, m, r, 1, "monday", "lunch").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].recipe_name, "Pancakes");
    assert!(rows[0].recipe_active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_menu_recipes_orders_by_week_day_meal(pool: PgPool) {
    let m = seed_menu(&pool, "Order Slots").await;
    let r = seed_recipe(&pool, "Soup").await;
    seed_slot(&pool, m, r, 2, "wednesday", "dinner").await;
    seed_slot(&pool, m, r, 1, "monday", "lunch").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    assert_eq!(rows[0].cycle_week, 1);
    assert_eq!(rows[1].cycle_week, 2);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/menus/:id/with-recipes
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_with_recipes_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        &format!("/api/menus/{BOGUS}/with-recipes"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_with_recipes_empty_slots_array(pool: PgPool) {
    let m = seed_menu(&pool, "No Slots WR").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuWithRecipesRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/with-recipes")).await)
            .await;
    assert!(row.recipe.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_with_recipes_includes_slots(pool: PgPool) {
    let m = seed_menu(&pool, "Slotted WR").await;
    let r = seed_recipe(&pool, "Chowder").await;
    seed_slot(&pool, m, r, 1, "monday", "lunch").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuWithRecipesRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/with-recipes")).await)
            .await;
    assert_eq!(row.recipe.len(), 1);
    assert_eq!(row.recipe[0].recipe_id, r);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/menus/with-recipes
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_recipes_empty_when_no_menus(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuWithRecipesRow> =
        decode_ok(get_admin(&server, &user.id, "/api/menus/with-recipes").await).await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_recipes_attaches_slots_per_menu(pool: PgPool) {
    let m1 = seed_menu(&pool, "WR M1").await;
    let m2 = seed_menu_dates(
        &pool,
        "WR M2",
        NaiveDate::from_ymd_opt(2025, 7, 1).unwrap(),
        NaiveDate::from_ymd_opt(2025, 7, 31).unwrap(),
        true,
    )
    .await;
    let r1 = seed_recipe(&pool, "WR R1").await;
    let r2 = seed_recipe(&pool, "WR R2").await;
    seed_slot(&pool, m1, r1, 1, "monday", "lunch").await;
    seed_slot(&pool, m2, r2, 1, "tuesday", "lunch").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuWithRecipesRow> =
        decode_ok(get_admin(&server, &user.id, "/api/menus/with-recipes").await).await;
    let row1 = rows.iter().find(|m| m.id == m1).expect("m1");
    let row2 = rows.iter().find(|m| m.id == m2).expect("m2");
    assert!(row1.recipe.iter().any(|s| s.recipe_id == r1));
    assert!(row2.recipe.iter().any(|s| s.recipe_id == r2));
    assert!(!row1.recipe.iter().any(|s| s.recipe_id == r2));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_recipes_excludes_inactive_by_default(pool: PgPool) {
    let inactive = seed_menu_inactive(&pool, "WR Hidden").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<MenuWithRecipesRow> =
        decode_ok(get_admin(&server, &user.id, "/api/menus/with-recipes").await).await;
    assert!(!rows.iter().any(|m| m.id == inactive));

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/menus/active-for-date
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn active_for_date_404_when_no_match(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/menus/active-for-date?date=2025-06-15",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn active_for_date_returns_covering_menu(pool: PgPool) {
    let id = seed_menu(&pool, "June").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/menus/active-for-date?date=2025-06-15",
        )
        .await,
    )
    .await;
    assert_eq!(row.id, id);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn active_for_date_404_when_inactive_even_if_in_range(pool: PgPool) {
    seed_menu_inactive(&pool, "June Inactive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/menus/active-for-date?date=2025-06-15",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn active_for_date_404_before_start_date(pool: PgPool) {
    seed_menu(&pool, "June Before").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/menus/active-for-date?date=2025-05-31",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn active_for_date_404_after_end_date(pool: PgPool) {
    seed_menu(&pool, "June After").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/menus/active-for-date?date=2025-07-01",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn active_for_date_boundaries_inclusive(pool: PgPool) {
    let id = seed_menu(&pool, "June Edges").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let start: MenuRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/menus/active-for-date?date=2025-06-01",
        )
        .await,
    )
    .await;
    let end: MenuRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/menus/active-for-date?date=2025-06-30",
        )
        .await,
    )
    .await;

    assert_eq!(start.id, id);
    assert_eq!(end.id, id);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/menus/:id/slots/by-coord
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_slot_404_for_missing(pool: PgPool) {
    let m = seed_menu(&pool, "Slot 404").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        &format!(
            "/api/menus/{m}/slots/by-coord?cycle_week=1&day_of_week=monday&meal_category=lunch"
        ),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_slot_returns_correct_row(pool: PgPool) {
    let m = seed_menu(&pool, "Slot Hit").await;
    let r = seed_recipe(&pool, "Casserole").await;
    seed_slot(&pool, m, r, 2, "friday", "dinner").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuRecipeWithNameRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/menus/{m}/slots/by-coord?cycle_week=2&day_of_week=friday&meal_category=dinner"),
        )
        .await,
    )
    .await;
    assert_eq!(row.recipe_id, r);
    assert_eq!(row.recipe_name, "Casserole");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_slot_404_for_different_day(pool: PgPool) {
    let m = seed_menu(&pool, "Diff Day").await;
    let r = seed_recipe(&pool, "Stew").await;
    seed_slot(&pool, m, r, 1, "monday", "lunch").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        &format!(
            "/api/menus/{m}/slots/by-coord?cycle_week=1&day_of_week=tuesday&meal_category=lunch"
        ),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// PATCH /api/menus/:id — update
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn update_name_only_leaves_other_fields(pool: PgPool) {
    let id = seed_menu(&pool, "Before Update").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuRow = decode_ok(
        patch_admin(
            &server,
            &user.id,
            &format!("/api/menus/{id}"),
            &json!({ "name": "After Update" }),
        )
        .await,
    )
    .await;
    assert_eq!(row.name, "After Update");
    assert!(row.is_active);
    assert_eq!(row.start_date, BASE_START);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_is_active_only(pool: PgPool) {
    let id = seed_menu(&pool, "Toggle Me").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuRow = decode_ok(
        patch_admin(
            &server,
            &user.id,
            &format!("/api/menus/{id}"),
            &json!({ "is_active": false }),
        )
        .await,
    )
    .await;
    assert!(!row.is_active);
    assert_eq!(row.name, "Toggle Me");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_trims_name(pool: PgPool) {
    let id = seed_menu(&pool, "Trim Target").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuRow = decode_ok(
        patch_admin(
            &server,
            &user.id,
            &format!("/api/menus/{id}"),
            &json!({ "name": "  Clean  " }),
        )
        .await,
    )
    .await;
    assert_eq!(row.name, "Clean");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/menus/{BOGUS}"),
        &json!({ "name": "Ghost" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/menus/:id/deactivate
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_sets_inactive(pool: PgPool) {
    let id = seed_menu(&pool, "Deact Me").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/menus/{id}/deactivate")).await,
    )
    .await;
    assert!(!row.is_active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_excludes_from_default_list(pool: PgPool) {
    let id = seed_menu(&pool, "Deact Hidden").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let _: MenuRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/menus/{id}/deactivate")).await,
    )
    .await;
    let rows: Vec<MenuRow> = decode_ok(get_admin(&server, &user.id, "/api/menus").await).await;
    assert!(!rows.iter().any(|r| r.id == id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_still_retrievable_with_include_inactive(pool: PgPool) {
    let id = seed_menu(&pool, "Deact Still").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let _: MenuRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/menus/{id}/deactivate")).await,
    )
    .await;
    let row: MenuRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/menus/{id}?include_inactive=true"),
        )
        .await,
    )
    .await;
    assert!(!row.is_active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/menus/{BOGUS}/deactivate")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/menus/:id/slots — set one slot (upsert)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn set_slot_inserts_new_row(pool: PgPool) {
    let m = seed_menu(&pool, "SS Menu").await;
    let r = seed_recipe(&pool, "SS Recipe").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: MenuRecipeRow = decode_ok(
        post_admin(
            &server,
            &user.id,
            &format!("/api/menus/{m}/slots"),
            &json!({
                "menu_id": m,
                "recipe_id": r,
                "cycle_week": 1,
                "day_of_week": "monday",
                "meal_category": "lunch",
            }),
        )
        .await,
    )
    .await;
    assert_eq!(row.recipe_id, r);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_slot_upserts_on_same_coord(pool: PgPool) {
    let m = seed_menu(&pool, "SS Upsert").await;
    let r1 = seed_recipe(&pool, "R1").await;
    let r2 = seed_recipe(&pool, "R2").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let path = format!("/api/menus/{m}/slots");
    let base = json!({
        "menu_id": m,
        "cycle_week": 1,
        "day_of_week": "monday",
        "meal_category": "lunch",
    });
    let mut first = base.clone();
    first["recipe_id"] = json!(r1);
    let mut second = base;
    second["recipe_id"] = json!(r2);

    let _: MenuRecipeRow = decode_ok(post_admin(&server, &user.id, &path, &first).await).await;
    let _: MenuRecipeRow = decode_ok(post_admin(&server, &user.id, &path, &second).await).await;

    let slots: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].recipe_id, r2);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_slot_allows_different_coords(pool: PgPool) {
    let m = seed_menu(&pool, "SS Many").await;
    let r = seed_recipe(&pool, "R").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let path = format!("/api/menus/{m}/slots");
    let _: MenuRecipeRow = decode_ok(
        post_admin(
            &server,
            &user.id,
            &path,
            &json!({
                "menu_id": m, "recipe_id": r, "cycle_week": 1,
                "day_of_week": "monday", "meal_category": "lunch",
            }),
        )
        .await,
    )
    .await;
    let _: MenuRecipeRow = decode_ok(
        post_admin(
            &server,
            &user.id,
            &path,
            &json!({
                "menu_id": m, "recipe_id": r, "cycle_week": 1,
                "day_of_week": "tuesday", "meal_category": "lunch",
            }),
        )
        .await,
    )
    .await;

    let slots: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    assert_eq!(slots.len(), 2);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/menus/:id/slots/clear
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn clear_slot_removes_existing_and_returns_true(pool: PgPool) {
    let m = seed_menu(&pool, "CS Rm").await;
    let r = seed_recipe(&pool, "CS R").await;
    seed_slot(&pool, m, r, 1, "monday", "lunch").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let v: Value = decode_ok(
        post_admin(
            &server,
            &user.id,
            &format!("/api/menus/{m}/slots/clear"),
            &json!({
                "cycle_week": 1, "day_of_week": "monday", "meal_category": "lunch",
            }),
        )
        .await,
    )
    .await;
    assert_eq!(v["cleared"], json!(true));

    let slots: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    assert!(slots.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn clear_slot_returns_false_for_missing(pool: PgPool) {
    let m = seed_menu(&pool, "CS Miss").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let v: Value = decode_ok(
        post_admin(
            &server,
            &user.id,
            &format!("/api/menus/{m}/slots/clear"),
            &json!({
                "cycle_week": 1, "day_of_week": "monday", "meal_category": "lunch",
            }),
        )
        .await,
    )
    .await;
    assert_eq!(v["cleared"], json!(false));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn clear_slot_leaves_other_slots_untouched(pool: PgPool) {
    let m = seed_menu(&pool, "CS Other").await;
    let r = seed_recipe(&pool, "CS R2").await;
    seed_slot(&pool, m, r, 1, "monday", "lunch").await;
    seed_slot(&pool, m, r, 1, "tuesday", "lunch").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let _: Value = decode_ok(
        post_admin(
            &server,
            &user.id,
            &format!("/api/menus/{m}/slots/clear"),
            &json!({
                "cycle_week": 1, "day_of_week": "monday", "meal_category": "lunch",
            }),
        )
        .await,
    )
    .await;

    let slots: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].day_of_week, "tuesday");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// PUT /api/menus/:id/slots — set all slots (atomic replace)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn set_all_slots_inserts_everything(pool: PgPool) {
    let m = seed_menu(&pool, "SA All").await;
    let r1 = seed_recipe(&pool, "SA R1").await;
    let r2 = seed_recipe(&pool, "SA R2").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = put_admin(
        &server,
        &user.id,
        &format!("/api/menus/{m}/slots"),
        &json!({
            "slots": [
                { "menu_id": m, "recipe_id": r1, "cycle_week": 1, "day_of_week": "monday", "meal_category": "lunch" },
                { "menu_id": m, "recipe_id": r2, "cycle_week": 1, "day_of_week": "tuesday", "meal_category": "lunch" },
            ],
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let slots: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    let ids: Vec<Uuid> = slots.iter().map(|s| s.recipe_id).collect();
    assert!(ids.contains(&r1));
    assert!(ids.contains(&r2));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_all_slots_replaces_on_second_call(pool: PgPool) {
    let m = seed_menu(&pool, "SA Replace").await;
    let old = seed_recipe(&pool, "SA Old").await;
    let next = seed_recipe(&pool, "SA Next").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let path = format!("/api/menus/{m}/slots");

    let _ = put_admin(
        &server,
        &user.id,
        &path,
        &json!({
            "slots": [
                { "menu_id": m, "recipe_id": old, "cycle_week": 1, "day_of_week": "monday", "meal_category": "lunch" },
            ],
        }),
    )
    .await;
    let _ = put_admin(
        &server,
        &user.id,
        &path,
        &json!({
            "slots": [
                { "menu_id": m, "recipe_id": next, "cycle_week": 1, "day_of_week": "wednesday", "meal_category": "dinner" },
            ],
        }),
    )
    .await;

    let slots: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    let ids: Vec<Uuid> = slots.iter().map(|s| s.recipe_id).collect();
    assert!(!ids.contains(&old));
    assert!(ids.contains(&next));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_all_slots_empty_array_clears(pool: PgPool) {
    let m = seed_menu(&pool, "SA Clear").await;
    let r = seed_recipe(&pool, "SA R").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let path = format!("/api/menus/{m}/slots");

    let _ = put_admin(
        &server,
        &user.id,
        &path,
        &json!({
            "slots": [
                { "menu_id": m, "recipe_id": r, "cycle_week": 1, "day_of_week": "monday", "meal_category": "lunch" },
            ],
        }),
    )
    .await;
    let _ = put_admin(&server, &user.id, &path, &json!({ "slots": [] })).await;

    let slots: Vec<MenuRecipeWithNameRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/menus/{m}/recipes")).await).await;
    assert!(slots.is_empty());

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Client scoping — reads unrestricted (global master data), writes 403.
// Rust-only; the TS suite can't express HTTP boundaries.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn client_can_read_list(pool: PgPool) {
    seed_menu(&pool, "Client Read List").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, Uuid::new_v4(), "/api/menus").await;
    assert_eq!(res.status(), StatusCode::OK);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_can_read_with_recipes(pool: PgPool) {
    seed_menu(&pool, "Client Read WR").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, Uuid::new_v4(), "/api/menus/with-recipes").await;
    assert_eq!(res.status(), StatusCode::OK);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_can_read_active_for_date(pool: PgPool) {
    seed_menu(&pool, "Client AFD").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        Uuid::new_v4(),
        "/api/menus/active-for-date?date=2025-06-15",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_create_forbidden(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        Uuid::new_v4(),
        "/api/menus",
        &json!({
            "name": "Client Attempt",
            "start_date": "2025-06-01",
            "end_date": "2025-06-30",
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_update_forbidden(pool: PgPool) {
    let id = seed_menu(&pool, "Client PATCH").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_client(
        &server,
        &user.id,
        Uuid::new_v4(),
        &format!("/api/menus/{id}"),
        &json!({ "name": "Nope" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_deactivate_forbidden(pool: PgPool) {
    let id = seed_menu(&pool, "Client Deact").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client_empty(
        &server,
        &user.id,
        Uuid::new_v4(),
        &format!("/api/menus/{id}/deactivate"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_set_slot_forbidden(pool: PgPool) {
    let m = seed_menu(&pool, "Client Slot").await;
    let r = seed_recipe(&pool, "Client R").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        Uuid::new_v4(),
        &format!("/api/menus/{m}/slots"),
        &json!({
            "menu_id": m, "recipe_id": r, "cycle_week": 1,
            "day_of_week": "monday", "meal_category": "lunch",
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_clear_slot_forbidden(pool: PgPool) {
    let m = seed_menu(&pool, "Client Clear").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        Uuid::new_v4(),
        &format!("/api/menus/{m}/slots/clear"),
        &json!({ "cycle_week": 1, "day_of_week": "monday", "meal_category": "lunch" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_set_all_slots_forbidden(pool: PgPool) {
    let m = seed_menu(&pool, "Client PUT").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = put_client(
        &server,
        &user.id,
        Uuid::new_v4(),
        &format!("/api/menus/{m}/slots"),
        &json!({ "slots": [] }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}
