use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BalanceEvent {
    Created { user_id: Uuid },
    Credited { user_id: Uuid, amount: Decimal },
    Debited { user_id: Uuid, amount: Decimal },
}

#[derive(Default)]
pub struct BalanceAggregate {
    pub user_id: Option<Uuid>,
    pub balance: Decimal,
}

impl BalanceAggregate {
    pub fn apply(&mut self, event: &BalanceEvent) {
        match event {
            BalanceEvent::Created { user_id } => {
                self.user_id = Some(*user_id);
                self.balance = Decimal::ZERO;
            }
            BalanceEvent::Credited { amount, .. } => {
                self.balance += amount;
            }
            BalanceEvent::Debited { amount, .. } => {
                self.balance -= amount;
            }
        }
    }

    pub fn load_from_history(events: &[BalanceEvent]) -> Self {
        let mut aggregate = Self::default();
        for event in events {
            aggregate.apply(event);
        }
        aggregate
    }

    pub fn handle_create(&self, user_id: Uuid) -> Result<BalanceEvent, String> {
        if self.user_id.is_some() {
            return Err("Balance already created".into());
        }
        Ok(BalanceEvent::Created { user_id })
    }

    pub fn handle_credit(&self, user_id: Uuid, amount: Decimal) -> Result<BalanceEvent, String> {
        if self.user_id.is_none() {
            return Err("Balance not created".into());
        }
        if amount <= Decimal::ZERO {
            return Err("Amount must be > 0".into());
        }
        Ok(BalanceEvent::Credited { user_id, amount })
    }

    pub fn handle_debit(&self, user_id: Uuid, amount: Decimal) -> Result<BalanceEvent, String> {
        if self.user_id.is_none() {
            return Err("Balance not created".into());
        }
        if amount > self.balance {
            return Err("Insufficient funds".into());
        }
        Ok(BalanceEvent::Debited { user_id, amount })
    }
}