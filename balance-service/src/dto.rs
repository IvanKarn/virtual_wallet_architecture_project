use rust_decimal::Decimal;
use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct AmountDto {
    pub amount: Decimal,
}