mod models;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use models::*;
use redis::AsyncCommands;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;
use std::time::Instant;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use validator::Validate;

use lapin::{options::*, types::FieldTable, BasicProperties, Connection, ConnectionProperties, ExchangeKind};
use shared_contract::{UserCreatedEvent, USER_EXCHANGE};

use metrics::{counter, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use sysinfo::{System, RefreshKind, CpuRefreshKind};

struct AppState {
    db: PgPool,
    redis: redis::aio::MultiplexedConnection,
    amqp_channel: lapin::Channel,
    prometheus_handle: PrometheusHandle,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/apidemo-db".to_string());
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());
    let rabbit_url = std::env::var("RABBITMQ_URL").unwrap_or_else(|_| "amqp://guest:guest@localhost:5672/%2f".to_string());

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

    let redis_client = redis::Client::open(redis_url).expect("Invalid Redis URL format");
    let redis_conn = loop {
        match redis_client.get_multiplexed_async_connection().await {
            Ok(conn) => {
                tracing::info!("Successfully connected to Redis");
                break conn;
            }
            Err(e) => {
                tracing::warn!("Failed to connect to Redis, retrying in 2s... Error: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
    };

    let amqp_conn = loop {
        match Connection::connect(&rabbit_url, ConnectionProperties::default()).await {
            Ok(conn) => {
                tracing::info!("Successfully connected to RabbitMQ");
                break conn;
            }
            Err(e) => {
                tracing::warn!("Failed to connect to RabbitMQ, retrying in 2s... Error: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
    };
    
    let amqp_channel = amqp_conn.create_channel().await.expect("Failed to create AMQP channel");

    amqp_channel
        .exchange_declare(
            USER_EXCHANGE.into(),
            ExchangeKind::Topic,
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("Failed to declare user.exchange");

    let recorder_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install metrics recorder");

    tokio::spawn(async move {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
        );
        loop {
            sys.refresh_cpu_usage();
            let cpu_usage = sys.global_cpu_usage() / 100.0;
            metrics::gauge!("system_cpu_usage").set(cpu_usage as f64);
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    });

    let state = Arc::new(AppState { 
        db: pool,
        redis: redis_conn,
        amqp_channel,
        prometheus_handle: recorder_handle,
    });

    #[derive(OpenApi)]
    #[openapi(
        paths(get_all_users, get_user, create_user, update_user, delete_user),
        components(schemas(UserDto, UserCreateDto, UserUpdateDto))
    )]
    struct ApiDoc;

    let app = Router::new()
        .route("/users", get(get_all_users).post(create_user))
        .route("/users/{id}", get(get_user).patch(update_user).delete(delete_user))
        .route("/actuator/prometheus", get(metrics_handler))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.expect("Failed to bind port 8080");
    tracing::info!("user-service listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    state.prometheus_handle.render()
}

#[utoipa::path(get, path = "/users", responses((status = 200, body = [UserDto])))]
async fn get_all_users(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let users = sqlx::query_as!(User, "SELECT * FROM users")
        .fetch_all(&state.db)
        .await
        .unwrap();

    let dtos: Vec<UserDto> = users.into_iter().map(UserDto::from).collect();
    (StatusCode::OK, Json(dtos))
}

#[utoipa::path(get, path = "/users/{id}", responses((status = 200, body = UserDto), (status = 404)))]
async fn get_user(
    Path(id): Path<Uuid>, 
    State(state): State<Arc<AppState>>
) -> Result<Json<UserDto>, StatusCode> {
    let start = Instant::now();

    let res = async {
        let cache_key = format!("user:{}", id);
        let mut redis_conn = state.redis.clone(); 

        let cached_user: Result<Option<String>, redis::RedisError> = redis_conn.get(&cache_key).await;

        if let Ok(Some(user_str)) = cached_user {
            if let Ok(user) = serde_json::from_str::<User>(&user_str) {
                tracing::info!("Cache hit for user: {}", id);
                return Ok(Json(user.into()));
            }
        }

        tracing::info!("Cache miss for user: {}. Fetching from DB...", id);

        let user = sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        match user {
            Some(u) => {
                if let Ok(user_str) = serde_json::to_string(&u) {
                    let _: () = redis_conn.set_ex(&cache_key, user_str, 3600).await.unwrap_or(());
                }
                Ok(Json(u.into()))
            }
            None => Err(StatusCode::NOT_FOUND),
        }
    }.await;

    let duration = start.elapsed().as_secs_f64();
    histogram!("api_user_request_duration_seconds", "method" => "get").record(duration);

    res
}

#[utoipa::path(post, path = "/users", request_body = UserCreateDto, responses((status = 201, body = UserDto)))]
async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UserCreateDto>,
) -> Result<(StatusCode, Json<UserDto>), StatusCode> {
    let start = Instant::now();

    let res = async {
        if payload.validate().is_err() {
            return Err(StatusCode::BAD_REQUEST);
        }

        let user = sqlx::query_as!(
            User,
            "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING *",
            payload.name,
            payload.email
        )
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        counter!("users_new_total").increment(1);

        let event = UserCreatedEvent {
            user_id: user.id,
            name: user.name.clone(),
            email: user.email.clone(),
        };

        if let Ok(payload_bytes) = serde_json::to_vec(&event) {
            let _ = state.amqp_channel.basic_publish(
                USER_EXCHANGE.into(),
                "user.created".into(),
                BasicPublishOptions::default(),
                &payload_bytes,
                BasicProperties::default().with_content_type("application/json".into()),
            ).await;
            tracing::info!("Sent UserCreatedEvent for id: {}", user.id);
        }

        Ok((StatusCode::CREATED, Json(user.into())))
    }.await;

    let duration = start.elapsed().as_secs_f64();
    histogram!("api_user_request_duration_seconds", "method" => "post").record(duration);

    res
}

#[utoipa::path(patch, path = "/users/{id}", request_body = UserUpdateDto, responses((status = 200, body = UserDto)))]
async fn update_user(
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UserUpdateDto>,
) -> Result<Json<UserDto>, StatusCode> {
    if payload.validate().is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut user = sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)
        .fetch_optional(&state.db)
        .await
        .unwrap()
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(name) = payload.name { user.name = name; }
    if let Some(email) = payload.email { user.email = email; }

    let updated = sqlx::query_as!(
        User,
        "UPDATE users SET name = $1, email = $2 WHERE id = $3 RETURNING *",
        user.name,
        user.email,
        id
    )
    .fetch_one(&state.db)
    .await
    .unwrap();

    let mut redis_conn = state.redis.clone();
    let cache_key = format!("user:{}", id);
    let _: () = redis_conn.del(&cache_key).await.unwrap_or(());

    Ok(Json(updated.into()))
}

#[utoipa::path(delete, path = "/users/{id}", responses((status = 204)))]
async fn delete_user(Path(id): Path<Uuid>, State(state): State<Arc<AppState>>) -> StatusCode {
    let result = sqlx::query!("DELETE FROM users WHERE id = $1", id)
        .execute(&state.db)
        .await
        .unwrap();

    if result.rows_affected() > 0 {
        let mut redis_conn = state.redis.clone();
        let cache_key = format!("user:{}", id);
        let _: () = redis_conn.del(&cache_key).await.unwrap_or(());

        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}