use rust_decimal::Decimal;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::domain::BalanceEvent;

pub async fn load_events(conn: &mut PgConnection, aggregate_id: Uuid) -> Result<Vec<BalanceEvent>, sqlx::Error> {
    let rows = sqlx::query("SELECT payload FROM event_store WHERE aggregate_id = $1 ORDER BY sequence_id ASC")
        .bind(aggregate_id)
        .fetch_all(&mut *conn)
        .await?;

    let mut events = Vec::new();
    for row in rows {
        let payload: serde_json::Value = row.get("payload");
        let event: BalanceEvent = serde_json::from_value(payload).unwrap();
        events.push(event);
    }

    Ok(events)
}

pub async fn save_event(conn: &mut PgConnection, aggregate_id: Uuid, event: &BalanceEvent) -> Result<(), sqlx::Error> {
    let event_type = match event {
        BalanceEvent::Created { .. } => "Created",
        BalanceEvent::Credited { .. } => "Credited",
        BalanceEvent::Debited { .. } => "Debited",
    };

    let payload = serde_json::to_value(event).unwrap();

    sqlx::query("INSERT INTO event_store (aggregate_id, event_type, payload) VALUES ($1, $2, $3)")
        .bind(aggregate_id)
        .bind(event_type)
        .bind(payload)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn project_event(conn: &mut PgConnection, event: &BalanceEvent) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now();
    match event {
        BalanceEvent::Created { user_id } => {
            sqlx::query("INSERT INTO balance_view (user_id, balance) VALUES ($1, $2)")
                .bind(user_id)
                .bind(Decimal::ZERO)
                .execute(&mut *conn)
                .await?;

            sqlx::query("INSERT INTO balance_history_entry (user_id, transaction_type, amount, timestamp) VALUES ($1, $2, $3, $4)")
                .bind(user_id)
                .bind("CREATE")
                .bind(Decimal::ZERO)
                .bind(now)
                .execute(&mut *conn)
                .await?;
        }
        BalanceEvent::Credited { user_id, amount } => {
            sqlx::query("UPDATE balance_view SET balance = balance + $1 WHERE user_id = $2")
                .bind(amount)
                .bind(user_id)
                .execute(&mut *conn)
                .await?;

            sqlx::query("INSERT INTO balance_history_entry (user_id, transaction_type, amount, timestamp) VALUES ($1, $2, $3, $4)")
                .bind(user_id)
                .bind("CREDIT")
                .bind(amount)
                .bind(now)
                .execute(&mut *conn)
                .await?;
        }
        BalanceEvent::Debited { user_id, amount } => {
            sqlx::query("UPDATE balance_view SET balance = balance - $1 WHERE user_id = $2")
                .bind(amount)
                .bind(user_id)
                .execute(&mut *conn)
                .await?;

            sqlx::query("INSERT INTO balance_history_entry (user_id, transaction_type, amount, timestamp) VALUES ($1, $2, $3, $4)")
                .bind(user_id)
                .bind("DEBIT")
                .bind(amount)
                .bind(now)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}