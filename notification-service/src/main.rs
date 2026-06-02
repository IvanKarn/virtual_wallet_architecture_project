use axum::Router;
use futures_util::stream::StreamExt;
use lapin::{options::*, types::FieldTable, Connection, ConnectionProperties, ExchangeKind};
use shared_contract::{UserCreatedEvent, USER_EXCHANGE, USER_QUEUE, USER_ROUTING_PATTERN};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let rabbit_url = std::env::var("RABBITMQ_URL").unwrap_or_else(|_| "amqp://guest:guest@localhost:5672/%2f".to_string());
    
    let amqp_conn = Connection::connect(&rabbit_url, ConnectionProperties::default())
        .await
        .expect("Failed to connect to RabbitMQ");
    let channel = amqp_conn.create_channel().await.expect("Failed to create AMQP channel");

    channel
        .exchange_declare(
            USER_EXCHANGE.into(),
            ExchangeKind::Topic,
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("Failed to declare exchange");

    channel
        .queue_declare(
            USER_QUEUE.into(),
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await
        .expect("Failed to declare queue");

    channel
        .queue_bind(
            USER_QUEUE.into(),
            USER_EXCHANGE.into(),
            USER_ROUTING_PATTERN.into(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("Failed to bind queue");

    let mut consumer = channel
        .basic_consume(
            USER_QUEUE.into(),
            "notification-service".into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("Failed to start consumer");

    tokio::spawn(async move {
        tracing::info!("RabbitMQ consumer started on queue: {}", USER_QUEUE);
        while let Some(delivery) = consumer.next().await {
            if let Ok(delivery) = delivery {
                if let Ok(event) = serde_json::from_slice::<UserCreatedEvent>(&delivery.data) {
                    handle_user_event(event).await;
                }
                let _ = delivery.ack(BasicAckOptions::default()).await;
            }
        }
    });

    let app = Router::new();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8081").await.unwrap();
    tracing::info!("notification-service listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn handle_user_event(event: UserCreatedEvent) {
    tracing::info!("Received event from RabbitMQ: {:?}", event);
    sleep(Duration::from_secs(2)).await;
    tracing::info!("Notification successfully sent for user: {}", event.name);
}