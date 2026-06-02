CREATE TABLE event_store (
    sequence_id BIGSERIAL PRIMARY KEY,
    aggregate_id UUID NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_event_store_aggregate_id ON event_store(aggregate_id);
CREATE INDEX idx_event_store_sequence_id ON event_store(sequence_id);

CREATE TABLE balance_view (
    user_id UUID PRIMARY KEY,
    balance NUMERIC NOT NULL
);

CREATE TABLE balance_history_entry (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL,
    transaction_type VARCHAR(50) NOT NULL,
    amount NUMERIC NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL
);

CREATE INDEX idx_balance_history_entry_user_id ON balance_history_entry(user_id);