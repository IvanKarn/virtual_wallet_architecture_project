use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, Row};
use utoipa::ToSchema;
use uuid::Uuid;


#[derive(Serialize, ToSchema)]
pub struct BalanceView {
    pub user_id: Uuid,
    pub balance: Decimal,
}

#[derive(Serialize, ToSchema)]
pub struct BalanceHistoryEntry {
    pub id: i64,
    pub user_id: Uuid,
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub amount: Decimal,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}


#[utoipa::path(
    get,
    path = "/balances/{user_id}",
    responses(
        (status = 200, description = "Get user balance", body = BalanceView),
        (status = 404, description = "Balance not found")
    ),
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    )
)]
pub async fn get_balance(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<BalanceView>, StatusCode> {
    let row = sqlx::query("SELECT user_id, balance FROM balance_view WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(r) = row {
        Ok(Json(BalanceView {
            user_id: r.get("user_id"),
            balance: r.get("balance"),
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[utoipa::path(
    get,
    path = "/balances/{user_id}/history",
    responses(
        (status = 200, description = "Get balance history", body = [BalanceHistoryEntry])
    ),
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    )
)]
pub async fn get_history(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Vec<BalanceHistoryEntry>>, StatusCode> {
    let rows = sqlx::query("SELECT id, user_id, transaction_type, amount, timestamp FROM balance_history_entry WHERE user_id = $1 ORDER BY timestamp ASC")
        .bind(user_id)
        .fetch_all(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let entries = rows.into_iter().map(|r| BalanceHistoryEntry {
        id: r.get("id"),
        user_id: r.get("user_id"),
        transaction_type: r.get("transaction_type"),
        amount: r.get("amount"),
        timestamp: r.get("timestamp"),
    }).collect();

    Ok(Json(entries))
}