CREATE TABLE IF NOT EXISTS transfers (
    id bigserial PRIMARY KEY,
    chat_id bigint NOT NULL REFERENCES Chats(id) ON DELETE CASCADE,
    from_uid bigint NOT NULL REFERENCES Users(uid) ON DELETE CASCADE,
    to_uid bigint NOT NULL REFERENCES Users(uid) ON DELETE CASCADE,
    amount integer NOT NULL CHECK (amount > 0),
    kind text NOT NULL CHECK (kind IN ('gift','fire')),
    created_at timestamptz NOT NULL DEFAULT current_timestamp
);

CREATE INDEX IF NOT EXISTS idx_transfers_from_uid ON transfers(from_uid);
CREATE INDEX IF NOT EXISTS idx_transfers_to_uid ON transfers(to_uid);
CREATE INDEX IF NOT EXISTS idx_transfers_chat_id ON transfers(chat_id);
