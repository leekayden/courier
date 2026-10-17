-- Calendar provider accounts (CalDAV / Google). The calendar stays disabled
-- until at least one account is configured (or legacy local events exist).
CREATE TABLE calendar_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL CHECK (provider IN ('caldav', 'google')),
    name TEXT NOT NULL DEFAULT '',
    color TEXT NOT NULL DEFAULT 'blue',
    url TEXT NOT NULL DEFAULT '',
    username TEXT NOT NULL DEFAULT '',
    secret TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1,
    last_synced_at TEXT,
    last_sync_error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Remote items are cached in calendar_events. Legacy local events keep
-- account_id NULL. Todos (VTODO / Google Tasks) are rows with
-- item_kind = 'todo' where start_time is DTSTART (may be empty) and
-- end_time is DUE (may be empty).
ALTER TABLE calendar_events ADD COLUMN account_id INTEGER REFERENCES calendar_accounts(id) ON DELETE CASCADE;
ALTER TABLE calendar_events ADD COLUMN item_kind TEXT NOT NULL DEFAULT 'event';
ALTER TABLE calendar_events ADD COLUMN remote_href TEXT;
ALTER TABLE calendar_events ADD COLUMN remote_etag TEXT;
ALTER TABLE calendar_events ADD COLUMN percent_complete INTEGER;
ALTER TABLE calendar_events ADD COLUMN priority INTEGER;
ALTER TABLE calendar_events ADD COLUMN completed_at TEXT;

CREATE INDEX idx_calendar_events_account ON calendar_events(account_id, item_kind);
