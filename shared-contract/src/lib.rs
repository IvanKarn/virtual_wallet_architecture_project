use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserCreatedEvent {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
}

pub const USER_EXCHANGE: &str = "user.exchange";
pub const USER_QUEUE: &str = "user.events";
pub const USER_ROUTING_KEY: &str = "user.created";
pub const USER_ROUTING_PATTERN: &str = "user.#";