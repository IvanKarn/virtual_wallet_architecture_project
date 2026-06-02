use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::BalanceAggregate;
use crate::dto::AmountDto;
use crate::store::{load_events, project_event, save_event};


pub async fn handle_create_command(pool: &PgPool, user_id: Uuid) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let events = load_events(&mut tx, user_id).await.map_err(|e| e.to_string())?;
    let aggregate = BalanceAggregate::load_from_history(&events);

    let event = aggregate.handle_create(user_id)?;

    save_event(&mut tx, user_id, &event).await.map_err(|e| e.to_string())?;
    project_event(&mut tx, &event).await.map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn handle_credit_command(pool: &PgPool, user_id: Uuid, amount: Decimal) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let events = load_events(&mut tx, user_id).await.map_err(|e| e.to_string())?;
    let aggregate = BalanceAggregate::load_from_history(&events);

    let event = aggregate.handle_credit(user_id, amount)?;

    save_event(&mut tx, user_id, &event).await.map_err(|e| e.to_string())?;
    project_event(&mut tx, &event).await.map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn handle_debit_command(pool: &PgPool, user_id: Uuid, amount: Decimal) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let events = load_events(&mut tx, user_id).await.map_err(|e| e.to_string())?;
    let aggregate = BalanceAggregate::load_from_history(&events);

    let event = aggregate.handle_debit(user_id, amount)?;

    save_event(&mut tx, user_id, &event).await.map_err(|e| e.to_string())?;
    project_event(&mut tx, &event).await.map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}


#[utoipa::path(
    post,
    path = "/balances/{user_id}/create",
    responses(
        (status = 201, description = "Balance created"),
        (status = 400, description = "Bad Request")
    ),
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    )
)]
pub async fn create_balance_api(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    handle_create_command(&pool, user_id).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    post,
    path = "/balances/{user_id}/credit",
    request_body = AmountDto,
    responses(
        (status = 200, description = "Balance credited"),
        (status = 400, description = "Bad Request")
    ),
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    )
)]
pub async fn credit_balance_api(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<AmountDto>,
) -> Result<StatusCode, (StatusCode, String)> {
    handle_credit_command(&pool, user_id, payload.amount).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::OK)
}

#[utoipa::path(
    post,
    path = "/balances/{user_id}/debit",
    request_body = AmountDto,
    responses(
        (status = 200, description = "Balance debited"),
        (status = 400, description = "Bad Request")
    ),
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    )
)]
pub async fn debit_balance_api(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<AmountDto>,
) -> Result<StatusCode, (StatusCode, String)> {
    handle_debit_command(&pool, user_id, payload.amount).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::OK)
}