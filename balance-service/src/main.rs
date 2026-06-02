mod commands;
mod domain;
mod dto;
mod queries;
mod store;

use axum::{
    routing::{get, post},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;


#[derive(OpenApi)]
#[openapi(
    paths(
        commands::create_balance_api,
        commands::credit_balance_api,
        commands::debit_balance_api,
        queries::get_balance,
        queries::get_history
    ),
    components(schemas(
        queries::BalanceView,
        queries::BalanceHistoryEntry,
        dto::AmountDto
    )) 
)]
struct ApiDoc;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("Balance service started (CQRS + Event Sourcing + Swagger)");

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/balance-db".to_string());

    let pool = loop {
        match PgPoolOptions::new().max_connections(5).connect(&database_url).await {
            Ok(p) => {
                tracing::info!("Successfully connected to Postgres");
                break p;
            }
            Err(e) => {
                tracing::warn!("Failed to connect to Postgres, retrying in 2s... Error: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
    };

    if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
        tracing::error!("Migration failed: {}", e);
        panic!("Migration failed");
    }

    let app = Router::new()
        .route("/balances/{user_id}/create", post(commands::create_balance_api))
        .route("/balances/{user_id}/credit", post(commands::credit_balance_api))
        .route("/balances/{user_id}/debit", post(commands::debit_balance_api))
        
        .route("/balances/{user_id}", get(queries::get_balance))
        .route("/balances/{user_id}/history", get(queries::get_history))
        
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.expect("Failed to bind port 8080");
    tracing::info!("balance-service listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}