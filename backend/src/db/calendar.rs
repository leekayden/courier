use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// A calendar event or todo record, mirroring the `calendar_events` table.
/// Todos are rows with `item_kind = "todo"` where `end_time` holds the DUE
/// value (possibly empty).
#[derive(Debug, Clone, Serialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub description: String,
    pub location: String,
    pub start_time: String,
    pub end_time: String,
    pub all_day: bool,
    pub recurrence_rule: Option<String>,
    pub attendees: String,
    pub organizer: String,
    pub status: String,
    pub source: String,
    pub source_uid: Option<String>,
    pub source_message_uid: Option<i64>,
    pub source_message_folder: Option<String>,
    pub meeting_url: Option<String>,
    pub color: Option<String>,
    pub reminder_minutes: Option<i64>,
    pub account_id: Option<i64>,
    pub account_name: Option<String>,
    pub item_kind: String,
    pub percent_complete: Option<i64>,
    pub priority: Option<i64>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A remotely-synced event, ready for the local cache. Produced by the
/// CalDAV and Google providers; never written back to the provider.
#[derive(Debug, Clone)]
pub struct RemoteEvent {
    pub uid: String,
    pub title: String,
    pub description: String,
    pub location: String,
    pub start_time: String,
    pub end_time: String,
    pub all_day: bool,
    pub recurrence_rule: Option<String>,
    /// JSON array of email addresses.
    pub attendees: String,
    pub organizer: String,
    pub status: String,
    pub meeting_url: Option<String>,
    pub remote_href: String,
    pub remote_etag: Option<String>,
}

/// A remotely-synced todo (CalDAV VTODO / Google Task).
#[derive(Debug, Clone)]
pub struct RemoteTodo {
    pub uid: String,
    pub title: String,
    pub description: String,
    /// DTSTART value; empty when the todo has no start date.
    pub start_time: String,
    /// DUE value; empty when the todo has no due date.
    pub due_time: String,
    pub status: String,
    pub completed: bool,
    pub completed_at: Option<String>,
    pub percent_complete: Option<i64>,
    pub priority: Option<i64>,
    pub url: Option<String>,
    pub remote_href: String,
    pub remote_etag: Option<String>,
}

/// Calendar account credentials as needed for syncing (includes the secret).
#[derive(Debug, Clone)]
pub struct AccountCredentials {
    pub id: i64,
    pub provider: String,
    pub color: String,
    pub url: String,
    pub username: String,
    pub secret: String,
}

/// A calendar account as exposed over the API (secret never included).
#[derive(Debug, Clone, Serialize)]
pub struct CalendarAccount {
    pub id: i64,
    pub provider: String,
    pub name: String,
    pub color: String,
    pub url: String,
    pub username: String,
    pub enabled: bool,
    pub has_secret: bool,
    pub last_synced_at: Option<String>,
    pub last_sync_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Body for POST /calendar/accounts.
#[derive(Debug, Deserialize)]
pub struct CreateCalendarAccount {
    /// "caldav" or "google".
    pub provider: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_account_color")]
    pub color: String,
    /// CalDAV: collection (or calendar home) URL. Google: calendar id ("primary").
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    /// CalDAV password / app password. Google: OAuth refresh token.
    #[serde(default)]
    pub secret: String,
}

fn default_account_color() -> String {
    "blue".to_string()
}

/// Body for PUT /calendar/accounts/{id}. Secret None = keep existing.
#[derive(Debug, Deserialize)]
pub struct UpdateCalendarAccount {
    pub name: Option<String>,
    pub color: Option<String>,
    pub url: Option<String>,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub enabled: Option<bool>,
}

/// Whether the calendar feature is usable for this user.
#[derive(Debug, Clone, Serialize)]
pub struct CalendarStatus {
    pub configured: bool,
    pub account_count: i64,
    pub enabled_account_count: i64,
    pub legacy_event_count: i64,
}

/// Calendar settings record, mirroring the `calendar_settings` table.
#[derive(Debug, Clone, Serialize)]
pub struct CalendarSettings {
    pub default_view: String,
    pub week_starts_on: i64,
    pub working_hours_start: String,
    pub working_hours_end: String,
    pub time_format: String,
    pub zoom_link: String,
    pub teams_link: String,
    pub meet_link: String,
    pub updated_at: String,
}

/// Fields for updating calendar settings. All optional.
#[derive(Debug, Deserialize)]
pub struct UpdateCalendarSettings {
    pub default_view: Option<String>,
    pub week_starts_on: Option<i64>,
    pub working_hours_start: Option<String>,
    pub working_hours_end: Option<String>,
    pub time_format: Option<String>,
    pub zoom_link: Option<String>,
    pub teams_link: Option<String>,
    pub meet_link: Option<String>,
}

/// A meeting template record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingTemplate {
    pub id: i64,
    pub name: String,
    pub url_template: String,
    pub icon: String,
    pub is_default: bool,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Row mappers
// ---------------------------------------------------------------------------

const EVENT_SELECT_COLS: &str = "e.id, e.title, e.description, e.location, e.start_time, \
    e.end_time, e.all_day, e.recurrence_rule, e.attendees, e.organizer, e.status, e.source, \
    e.source_uid, e.source_message_uid, e.source_message_folder, e.meeting_url, e.color, \
    e.reminder_minutes, e.account_id, a.name, e.item_kind, e.percent_complete, e.priority, \
    e.completed_at, e.created_at, e.updated_at";

const EVENT_FROM: &str =
    "calendar_events e LEFT JOIN calendar_accounts a ON a.id = e.account_id";

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<CalendarEvent> {
    let all_day_int: i32 = row.get(6)?;
    Ok(CalendarEvent {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        location: row.get(3)?,
        start_time: row.get(4)?,
        end_time: row.get(5)?,
        all_day: all_day_int != 0,
        recurrence_rule: row.get(7)?,
        attendees: row.get(8)?,
        organizer: row.get(9)?,
        status: row.get(10)?,
        source: row.get(11)?,
        source_uid: row.get(12)?,
        source_message_uid: row.get(13)?,
        source_message_folder: row.get(14)?,
        meeting_url: row.get(15)?,
        color: row.get(16)?,
        reminder_minutes: row.get(17)?,
        account_id: row.get(18)?,
        account_name: row.get(19)?,
        item_kind: row.get(20)?,
        percent_complete: row.get(21)?,
        priority: row.get(22)?,
        completed_at: row.get(23)?,
        created_at: row.get(24)?,
        updated_at: row.get(25)?,
    })
}

const ACCOUNT_SELECT_COLS: &str =
    "id, provider, name, color, url, username, secret, enabled, last_synced_at, \
     last_sync_error, created_at, updated_at";

fn row_to_account(row: &rusqlite::Row<'_>) -> rusqlite::Result<CalendarAccount> {
    let secret: String = row.get(6)?;
    let enabled_int: i32 = row.get(7)?;
    Ok(CalendarAccount {
        id: row.get(0)?,
        provider: row.get(1)?,
        name: row.get(2)?,
        color: row.get(3)?,
        url: row.get(4)?,
        username: row.get(5)?,
        enabled: enabled_int != 0,
        has_secret: !secret.is_empty(),
        last_synced_at: row.get(8)?,
        last_sync_error: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn row_to_credentials(row: &rusqlite::Row<'_>) -> rusqlite::Result<AccountCredentials> {
    Ok(AccountCredentials {
        id: row.get(0)?,
        provider: row.get(1)?,
        color: row.get(3)?,
        url: row.get(4)?,
        username: row.get(5)?,
        secret: row.get(6)?,
    })
}

fn row_to_settings(row: &rusqlite::Row<'_>) -> rusqlite::Result<CalendarSettings> {
    Ok(CalendarSettings {
        default_view: row.get(0)?,
        week_starts_on: row.get(1)?,
        working_hours_start: row.get(2)?,
        working_hours_end: row.get(3)?,
        time_format: row.get(4)?,
        zoom_link: row.get(5)?,
        teams_link: row.get(6)?,
        meet_link: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn row_to_template(row: &rusqlite::Row<'_>) -> rusqlite::Result<MeetingTemplate> {
    let is_default_int: i32 = row.get(4)?;
    Ok(MeetingTemplate {
        id: row.get(0)?,
        name: row.get(1)?,
        url_template: row.get(2)?,
        icon: row.get(3)?,
        is_default: is_default_int != 0,
        created_at: row.get(5)?,
    })
}

// ---------------------------------------------------------------------------
// Public API — Events
// ---------------------------------------------------------------------------

/// List events in the given time range (inclusive start, inclusive end).
pub fn list_events(
    conn: &Connection,
    start: &str,
    end: &str,
) -> Result<Vec<CalendarEvent>, String> {
    let sql = format!(
        "SELECT {EVENT_SELECT_COLS} FROM {EVENT_FROM}
         WHERE e.item_kind = 'event' AND e.end_time >= ?1 AND e.start_time <= ?2
         ORDER BY e.start_time ASC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare list_events: {e}"))?;
    let rows = stmt
        .query_map(params![start, end], row_to_event)
        .map_err(|e| format!("Failed to query events: {e}"))?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|e| format!("Failed to read event row: {e}"))?);
    }
    Ok(expand_recurring(events, start, end))
}

/// List all todos (VTODO / Google Tasks cache). Todos are usually few, so no
/// time filtering here — the UI buckets them (overdue / due / no date).
pub fn list_todos(conn: &Connection) -> Result<Vec<CalendarEvent>, String> {
    let sql = format!(
        "SELECT {EVENT_SELECT_COLS} FROM {EVENT_FROM}
         WHERE e.item_kind = 'todo'
         ORDER BY CASE WHEN e.end_time = '' THEN 1 ELSE 0 END, e.end_time ASC, e.title ASC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare list_todos: {e}"))?;
    let rows = stmt
        .query_map([], row_to_event)
        .map_err(|e| format!("Failed to query todos: {e}"))?;

    let mut todos = Vec::new();
    for row in rows {
        todos.push(row.map_err(|e| format!("Failed to read todo row: {e}"))?);
    }
    Ok(todos)
}

// ---------------------------------------------------------------------------
// Recurring event expansion
// ---------------------------------------------------------------------------

/// Expand recurring events within [range_start, range_end].
///
/// Events without a recurrence_rule pass through unchanged. Events with one are
/// replaced by their individual occurrences inside the window. On any parse or
/// expansion error the master event is kept as-is.
fn expand_recurring(
    events: Vec<CalendarEvent>,
    range_start: &str,
    range_end: &str,
) -> Vec<CalendarEvent> {
    let mut out: Vec<CalendarEvent> = Vec::with_capacity(events.len());

    for event in events {
        match try_expand(&event, range_start, range_end) {
            Ok(occurrences) => out.extend(occurrences),
            Err(_) => out.push(event),
        }
    }

    out.sort_by(|a, b| a.start_time.cmp(&b.start_time));
    out
}

fn try_expand(
    event: &CalendarEvent,
    range_start: &str,
    range_end: &str,
) -> Result<Vec<CalendarEvent>, Box<dyn std::error::Error>> {
    let rule_str = match &event.recurrence_rule {
        Some(r) if !r.is_empty() => r,
        _ => return Ok(vec![event.clone()]),
    };

    let is_date_only = event.start_time.len() == 10 && !event.start_time.contains('T');

    // Build the iCal string the rrule crate needs.
    let dtstart = iso_to_ical_dtstart(&event.start_time);
    let ical_str = format!("{dtstart}\nRRULE:{rule_str}");

    let rrule_set: rrule::RRuleSet = ical_str.parse()?;

    // Parse range bounds as UTC datetimes for comparison.
    let window_start = parse_iso_as_utc(range_start)?;
    let window_end = parse_iso_as_utc(range_end)?;

    // Calculate duration so we can derive end_time per occurrence.
    let duration = if is_date_only {
        let s = NaiveDate::parse_from_str(&event.start_time, "%Y-%m-%d")?;
        let e = NaiveDate::parse_from_str(&event.end_time, "%Y-%m-%d")?;
        Duration::days((e - s).num_days())
    } else {
        let s = parse_iso_as_utc(&event.start_time)?;
        let e = parse_iso_as_utc(&event.end_time)?;
        e - s
    };

    let occurrences: Vec<CalendarEvent> = rrule_set
        .into_iter()
        .skip_while(|dt| dt.with_timezone(&Utc) < window_start)
        .take_while(|dt| dt.with_timezone(&Utc) <= window_end)
        .take(500)
        .map(|dt| {
            let utc = dt.with_timezone(&Utc);
            let (occ_start, occ_end) = if is_date_only {
                let d = utc.date_naive();
                let end_d = d + duration;
                (d.format("%Y-%m-%d").to_string(), end_d.format("%Y-%m-%d").to_string())
            } else {
                let end_utc = utc + duration;
                (
                    utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                    end_utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                )
            };
            let mut occ = event.clone();
            occ.start_time = occ_start;
            occ.end_time = occ_end;
            occ
        })
        .collect();

    // If expansion produced nothing (master start outside range), return master.
    if occurrences.is_empty() {
        return Ok(vec![event.clone()]);
    }

    Ok(occurrences)
}

/// Convert an ISO 8601 datetime/date string to an iCal DTSTART line.
fn iso_to_ical_dtstart(iso: &str) -> String {
    if iso.len() == 10 && !iso.contains('T') {
        // Date-only: 2025-03-06 -> DTSTART;VALUE=DATE:20250306
        let compact = iso.replace('-', "");
        format!("DTSTART;VALUE=DATE:{compact}")
    } else {
        // Datetime: 2025-03-06T09:00:00Z -> DTSTART:20250306T090000Z
        let compact = iso.replace(['-', ':'], "");
        // The replace above turns colons in the time part to nothing; fix the T.
        // e.g. "20250306T090000Z"
        format!("DTSTART:{compact}")
    }
}

/// Parse an ISO 8601 string (datetime or date-only) as a UTC DateTime.
fn parse_iso_as_utc(iso: &str) -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
    if iso.len() == 10 && !iso.contains('T') {
        let d = NaiveDate::parse_from_str(iso, "%Y-%m-%d")?;
        Ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc())
    } else if iso.ends_with('Z') {
        Ok(iso.parse::<DateTime<Utc>>()?)
    } else {
        // Local datetime without timezone - treat as UTC.
        let naive = chrono::NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S")?;
        Ok(naive.and_utc())
    }
}

/// Get a single event or todo by ID.
pub fn get_event(conn: &Connection, id: &str) -> Result<Option<CalendarEvent>, String> {
    let sql = format!("SELECT {EVENT_SELECT_COLS} FROM {EVENT_FROM} WHERE e.id = ?1");
    let result = conn.query_row(&sql, params![id], row_to_event);

    match result {
        Ok(e) => Ok(Some(e)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get event: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Public API — Remote item cache
// ---------------------------------------------------------------------------

/// Replace the cached items for one account with the freshly synced set.
/// Runs in a transaction: readers never see a half-applied sync.
pub fn replace_account_items(
    conn: &Connection,
    account_id: i64,
    provider: &str,
    color: &str,
    events: &[RemoteEvent],
    todos: &[RemoteTodo],
) -> Result<usize, String> {
    let now: String = conn
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Failed to get current time: {e}"))?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("Failed to start transaction: {e}"))?;

    tx.execute("DELETE FROM calendar_events WHERE account_id = ?1", params![account_id])
        .map_err(|e| format!("Failed to clear account cache: {e}"))?;

    let mut inserted = 0usize;

    for event in events {
        tx.execute(
            "INSERT INTO calendar_events
                (id, title, description, location, start_time, end_time, all_day,
                 recurrence_rule, attendees, organizer, status, source, source_uid,
                 meeting_url, color, account_id, item_kind, remote_href, remote_etag,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 'event', ?17, ?18, ?19, ?19)",
            params![
                Uuid::new_v4().to_string(),
                event.title,
                event.description,
                event.location,
                event.start_time,
                event.end_time,
                event.all_day as i32,
                event.recurrence_rule,
                event.attendees,
                event.organizer,
                event.status,
                provider,
                event.uid,
                event.meeting_url,
                color,
                account_id,
                event.remote_href,
                event.remote_etag,
                now,
            ],
        )
        .map_err(|e| format!("Failed to cache remote event: {e}"))?;
        inserted += 1;
    }

    for todo in todos {
        tx.execute(
            "INSERT INTO calendar_events
                (id, title, description, start_time, end_time, all_day, attendees,
                 organizer, status, source, source_uid, meeting_url, color, account_id,
                 item_kind, remote_href, remote_etag, percent_complete, priority,
                 completed_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, '[]', '', ?6, ?7, ?8, ?9, ?10, ?11,
                     'todo', ?12, ?13, ?14, ?15, ?16, ?17, ?17)",
            params![
                Uuid::new_v4().to_string(),
                todo.title,
                todo.description,
                todo.start_time,
                todo.due_time,
                if todo.completed { "completed" } else { todo.status.as_str() },
                provider,
                todo.uid,
                todo.url,
                color,
                account_id,
                todo.remote_href,
                todo.remote_etag,
                todo.percent_complete,
                todo.priority,
                todo.completed_at,
                now,
            ],
        )
        .map_err(|e| format!("Failed to cache remote todo: {e}"))?;
        inserted += 1;
    }

    tx.commit()
        .map_err(|e| format!("Failed to commit account cache: {e}"))?;

    Ok(inserted)
}

// ---------------------------------------------------------------------------
// Public API — Accounts
// ---------------------------------------------------------------------------

/// List all configured calendar accounts (secrets redacted).
pub fn list_accounts(conn: &Connection) -> Result<Vec<CalendarAccount>, String> {
    let sql = format!(
        "SELECT {ACCOUNT_SELECT_COLS} FROM calendar_accounts ORDER BY created_at ASC, id ASC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare list_accounts: {e}"))?;
    let rows = stmt
        .query_map([], row_to_account)
        .map_err(|e| format!("Failed to query accounts: {e}"))?;

    let mut accounts = Vec::new();
    for row in rows {
        accounts.push(row.map_err(|e| format!("Failed to read account row: {e}"))?);
    }
    Ok(accounts)
}

/// List enabled account credentials for syncing.
pub fn list_enabled_credentials(conn: &Connection) -> Result<Vec<AccountCredentials>, String> {
    let sql = format!(
        "SELECT {ACCOUNT_SELECT_COLS} FROM calendar_accounts WHERE enabled = 1 ORDER BY id ASC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare list_enabled_credentials: {e}"))?;
    let rows = stmt
        .query_map([], row_to_credentials)
        .map_err(|e| format!("Failed to query credentials: {e}"))?;

    let mut accounts = Vec::new();
    for row in rows {
        accounts.push(row.map_err(|e| format!("Failed to read credential row: {e}"))?);
    }
    Ok(accounts)
}

/// Create a calendar account. Returns the redacted account.
pub fn create_account(
    conn: &Connection,
    data: &CreateCalendarAccount,
) -> Result<CalendarAccount, String> {
    conn.execute(
        "INSERT INTO calendar_accounts (provider, name, color, url, username, secret)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![data.provider, data.name, data.color, data.url, data.username, data.secret],
    )
    .map_err(|e| format!("Failed to create calendar account: {e}"))?;

    let id = conn.last_insert_rowid();
    get_account(conn, id)?.ok_or_else(|| "Account not found after insert".to_string())
}

/// Get one account (redacted) by ID.
pub fn get_account(conn: &Connection, id: i64) -> Result<Option<CalendarAccount>, String> {
    let sql = format!("SELECT {ACCOUNT_SELECT_COLS} FROM calendar_accounts WHERE id = ?1");
    let result = conn.query_row(&sql, params![id], row_to_account);
    match result {
        Ok(a) => Ok(Some(a)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get account: {e}")),
    }
}

/// Update an account. Secret None = keep the existing secret.
/// Returns the updated redacted account, or None if not found.
pub fn update_account(
    conn: &Connection,
    id: i64,
    data: &UpdateCalendarAccount,
) -> Result<Option<CalendarAccount>, String> {
    let mut sets = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1u32;

    macro_rules! maybe_set {
        ($field:ident, $col:expr) => {
            if let Some(ref v) = data.$field {
                sets.push(format!("{} = ?{idx}", $col));
                values.push(Box::new(v.clone()));
                idx += 1;
            }
        };
    }

    maybe_set!(name, "name");
    maybe_set!(color, "color");
    maybe_set!(url, "url");
    maybe_set!(username, "username");

    if let Some(ref secret) = data.secret {
        sets.push(format!("secret = ?{idx}"));
        values.push(Box::new(secret.clone()));
        idx += 1;
    }

    if let Some(enabled) = data.enabled {
        sets.push(format!("enabled = ?{idx}"));
        values.push(Box::new(enabled as i32));
        idx += 1;
    }

    if !sets.is_empty() {
        sets.push("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')".to_string());
        sets.push("last_sync_error = NULL".to_string());
        let sql = format!("UPDATE calendar_accounts SET {} WHERE id = ?{idx}", sets.join(", "));
        values.push(Box::new(id));
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            values.iter().map(|b| b.as_ref()).collect();
        let updated = conn
            .execute(&sql, params_refs.as_slice())
            .map_err(|e| format!("Failed to update calendar account: {e}"))?;
        if updated == 0 {
            return Ok(None);
        }
    }

    get_account(conn, id)
}

/// Delete an account and its cached items (FK cascade). Returns true if a row
/// was deleted.
pub fn delete_account(conn: &Connection, id: i64) -> Result<bool, String> {
    let deleted = conn
        .execute("DELETE FROM calendar_accounts WHERE id = ?1", params![id])
        .map_err(|e| format!("Failed to delete calendar account: {e}"))?;
    Ok(deleted > 0)
}

/// Record the outcome of a sync attempt on an account.
pub fn set_account_sync_state(
    conn: &Connection,
    id: i64,
    error: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE calendar_accounts
         SET last_synced_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), last_sync_error = ?2
         WHERE id = ?1",
        params![id, error],
    )
    .map_err(|e| format!("Failed to record sync state: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API — Status
// ---------------------------------------------------------------------------

/// Whether the calendar feature is usable: at least one enabled account, or
/// legacy local events from before the calendar became provider-backed.
pub fn get_calendar_status(conn: &Connection) -> Result<CalendarStatus, String> {
    let account_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM calendar_accounts", [], |row| row.get(0))
        .map_err(|e| format!("Failed to count accounts: {e}"))?;
    let enabled_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM calendar_accounts WHERE enabled = 1", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Failed to count enabled accounts: {e}"))?;
    let legacy_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM calendar_events WHERE account_id IS NULL AND item_kind = 'event'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to count legacy events: {e}"))?;

    Ok(CalendarStatus {
        configured: enabled_count > 0 || legacy_count > 0,
        account_count,
        enabled_account_count: enabled_count,
        legacy_event_count: legacy_count,
    })
}

// ---------------------------------------------------------------------------
// Public API — Settings
// ---------------------------------------------------------------------------

const SETTINGS_SELECT_COLS: &str = "default_view, week_starts_on, working_hours_start, \
    working_hours_end, time_format, zoom_link, teams_link, meet_link, updated_at";

/// Get calendar settings, inserting a default row if none exists.
pub fn get_calendar_settings(conn: &Connection) -> Result<CalendarSettings, String> {
    // Ensure the singleton row exists.
    conn.execute(
        "INSERT OR IGNORE INTO calendar_settings (id) VALUES (1)",
        [],
    )
    .map_err(|e| format!("Failed to ensure calendar_settings row: {e}"))?;

    let sql = format!(
        "SELECT {SETTINGS_SELECT_COLS} FROM calendar_settings WHERE id = 1"
    );
    conn.query_row(&sql, [], row_to_settings)
        .map_err(|e| format!("Failed to get calendar settings: {e}"))
}

/// Update calendar settings. Returns the updated settings.
pub fn update_calendar_settings(
    conn: &Connection,
    data: &UpdateCalendarSettings,
) -> Result<CalendarSettings, String> {
    // Ensure the singleton row exists.
    conn.execute(
        "INSERT OR IGNORE INTO calendar_settings (id) VALUES (1)",
        [],
    )
    .map_err(|e| format!("Failed to ensure calendar_settings row: {e}"))?;

    let mut sets = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1u32;

    macro_rules! maybe_set {
        ($field:ident, $col:expr) => {
            if let Some(ref v) = data.$field {
                sets.push(format!("{} = ?{idx}", $col));
                values.push(Box::new(v.clone()));
                idx += 1;
            }
        };
    }

    maybe_set!(default_view, "default_view");
    maybe_set!(working_hours_start, "working_hours_start");
    maybe_set!(working_hours_end, "working_hours_end");
    maybe_set!(time_format, "time_format");
    maybe_set!(zoom_link, "zoom_link");
    maybe_set!(teams_link, "teams_link");
    maybe_set!(meet_link, "meet_link");

    if let Some(v) = data.week_starts_on {
        sets.push(format!("week_starts_on = ?{idx}"));
        values.push(Box::new(v));
    }

    if !sets.is_empty() {
        sets.push("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')".to_string());
        let sql = format!(
            "UPDATE calendar_settings SET {} WHERE id = 1",
            sets.join(", ")
        );
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            values.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())
            .map_err(|e| format!("Failed to update calendar settings: {e}"))?;
    }

    get_calendar_settings(conn)
}

// ---------------------------------------------------------------------------
// Public API — Meeting Templates
// ---------------------------------------------------------------------------

/// List all meeting templates.
pub fn list_meeting_templates(conn: &Connection) -> Result<Vec<MeetingTemplate>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, url_template, icon, is_default, created_at
             FROM meeting_templates ORDER BY is_default DESC, name ASC",
        )
        .map_err(|e| format!("Failed to prepare list_meeting_templates: {e}"))?;
    let rows = stmt
        .query_map([], row_to_template)
        .map_err(|e| format!("Failed to query meeting templates: {e}"))?;

    let mut templates = Vec::new();
    for row in rows {
        templates.push(row.map_err(|e| format!("Failed to read template row: {e}"))?);
    }
    Ok(templates)
}

/// Create a meeting template.
pub fn create_meeting_template(
    conn: &Connection,
    name: &str,
    url_template: &str,
    icon: &str,
) -> Result<MeetingTemplate, String> {
    conn.execute(
        "INSERT INTO meeting_templates (name, url_template, icon) VALUES (?1, ?2, ?3)",
        params![name, url_template, icon],
    )
    .map_err(|e| format!("Failed to create meeting template: {e}"))?;

    let id = conn.last_insert_rowid();
    let sql = "SELECT id, name, url_template, icon, is_default, created_at
               FROM meeting_templates WHERE id = ?1";
    conn.query_row(sql, params![id], row_to_template)
        .map_err(|e| format!("Failed to read created template: {e}"))
}

/// Delete a meeting template by ID. Returns true if a row was deleted.
pub fn delete_meeting_template(conn: &Connection, id: i64) -> Result<bool, String> {
    let deleted = conn
        .execute("DELETE FROM meeting_templates WHERE id = ?1", params![id])
        .map_err(|e| format!("Failed to delete meeting template: {e}"))?;
    Ok(deleted > 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::pool::open_test_db;

    fn remote_event(uid: &str, title: &str, start: &str, end: &str) -> RemoteEvent {
        RemoteEvent {
            uid: uid.to_string(),
            title: title.to_string(),
            description: String::new(),
            location: String::new(),
            start_time: start.to_string(),
            end_time: end.to_string(),
            all_day: false,
            recurrence_rule: None,
            attendees: "[]".to_string(),
            organizer: String::new(),
            status: "confirmed".to_string(),
            meeting_url: None,
            remote_href: format!("/cal/{uid}.ics"),
            remote_etag: Some("\"abc\"".to_string()),
        }
    }

    fn remote_todo(uid: &str, title: &str, due: &str) -> RemoteTodo {
        RemoteTodo {
            uid: uid.to_string(),
            title: title.to_string(),
            description: String::new(),
            start_time: String::new(),
            due_time: due.to_string(),
            status: "needs-action".to_string(),
            completed: false,
            completed_at: None,
            percent_complete: Some(0),
            priority: Some(5),
            url: None,
            remote_href: format!("/cal/{uid}.ics"),
            remote_etag: None,
        }
    }

    fn account() -> CreateCalendarAccount {
        CreateCalendarAccount {
            provider: "caldav".to_string(),
            name: "Baikal".to_string(),
            color: "blue".to_string(),
            url: "https://cal.example.com/dav.php/calendars/kayden/default/".to_string(),
            username: "kayden".to_string(),
            secret: "hunter2".to_string(),
        }
    }

    #[test]
    fn test_account_crud_and_secret_redaction() {
        let conn = open_test_db();

        let created = create_account(&conn, &account()).unwrap();
        assert_eq!(created.provider, "caldav");
        assert_eq!(created.username, "kayden");
        assert!(created.has_secret);

        // The API struct never carries the secret.
        let json = serde_json::to_string(&created).unwrap();
        assert!(!json.contains("hunter2"));

        let update = UpdateCalendarAccount {
            name: Some("My Baikal".to_string()),
            color: None,
            url: None,
            username: None,
            secret: None, // keep existing
            enabled: Some(true),
        };
        let updated = update_account(&conn, created.id, &update).unwrap().unwrap();
        assert_eq!(updated.name, "My Baikal");
        assert!(updated.enabled);
        assert!(updated.has_secret);

        let creds = list_enabled_credentials(&conn).unwrap();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].secret, "hunter2");
        assert_eq!(creds[0].username, "kayden");

        assert!(delete_account(&conn, created.id).unwrap());
        assert!(!delete_account(&conn, created.id).unwrap());
    }

    #[test]
    fn test_status_reflects_accounts_and_legacy_events() {
        let conn = open_test_db();

        let status = get_calendar_status(&conn).unwrap();
        assert!(!status.configured);

        // Accounts are enabled when created, so the calendar turns on.
        let created = create_account(&conn, &account()).unwrap();
        let status = get_calendar_status(&conn).unwrap();
        assert!(status.configured);
        assert_eq!(status.enabled_account_count, 1);

        // Disabling the only account disables the calendar again.
        update_account(
            &conn,
            created.id,
            &UpdateCalendarAccount {
                name: None,
                color: None,
                url: None,
                username: None,
                secret: None,
                enabled: Some(false),
            },
        )
        .unwrap()
        .unwrap();
        let status = get_calendar_status(&conn).unwrap();
        assert!(!status.configured);
    }

    #[test]
    fn test_replace_account_items_events_and_todos() {
        let conn = open_test_db();
        let created = create_account(&conn, &account()).unwrap();

        let events = vec![
            remote_event("uid-1@example.com", "Standup", "2026-03-02T09:00:00Z", "2026-03-02T09:30:00Z"),
            remote_event("uid-2@example.com", "Retro", "2026-03-06T15:00:00Z", "2026-03-06T16:00:00Z"),
        ];
        let todos = vec![remote_todo("todo-1@example.com", "Send invoice", "2026-03-04T17:00:00Z")];

        replace_account_items(&conn, created.id, "caldav", "blue", &events, &todos).unwrap();

        let listed = list_events(&conn, "2026-03-01T00:00:00Z", "2026-03-31T23:59:59Z").unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].title, "Standup");
        assert_eq!(listed[0].source, "caldav");
        assert_eq!(listed[0].account_id, Some(created.id));
        assert_eq!(listed[0].account_name.as_deref(), Some("Baikal"));
        assert_eq!(listed[0].color.as_deref(), Some("blue"));

        let todos = list_todos(&conn).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].item_kind, "todo");
        assert_eq!(todos[0].end_time, "2026-03-04T17:00:00Z");
        assert_eq!(todos[0].status, "needs-action");
        assert_eq!(todos[0].priority, Some(5));

        // A second sync replaces the cache rather than duplicating it.
        let events2 = vec![remote_event("uid-1@example.com", "Standup renamed", "2026-03-02T09:00:00Z", "2026-03-02T09:30:00Z")];
        replace_account_items(&conn, created.id, "caldav", "blue", &events2, &[]).unwrap();
        let listed = list_events(&conn, "2026-03-01T00:00:00Z", "2026-03-31T23:59:59Z").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Standup renamed");
        assert!(list_todos(&conn).unwrap().is_empty());
    }

    #[test]
    fn test_delete_account_cascades_cached_items() {
        let conn = open_test_db();
        let created = create_account(&conn, &account()).unwrap();
        replace_account_items(
            &conn,
            created.id,
            "caldav",
            "blue",
            &[remote_event("u1@x", "E", "2026-03-02T09:00:00Z", "2026-03-02T10:00:00Z")],
            &[],
        )
        .unwrap();

        delete_account(&conn, created.id).unwrap();
        assert!(list_events(&conn, "2026-03-01T00:00:00Z", "2026-03-31T23:59:59Z")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_sync_state_roundtrip() {
        let conn = open_test_db();
        let created = create_account(&conn, &account()).unwrap();

        set_account_sync_state(&conn, created.id, None).unwrap();
        let acct = get_account(&conn, created.id).unwrap().unwrap();
        assert!(acct.last_synced_at.is_some());
        assert!(acct.last_sync_error.is_none());

        set_account_sync_state(&conn, created.id, Some("boom")).unwrap();
        let acct = get_account(&conn, created.id).unwrap().unwrap();
        assert_eq!(acct.last_sync_error.as_deref(), Some("boom"));
    }

    #[test]
    fn test_calendar_settings() {
        let conn = open_test_db();

        let settings = get_calendar_settings(&conn).unwrap();
        assert_eq!(settings.default_view, "month");
        assert_eq!(settings.week_starts_on, 0);
        assert_eq!(settings.time_format, "12h");

        let update = UpdateCalendarSettings {
            default_view: Some("week".to_string()),
            week_starts_on: Some(1),
            working_hours_start: None,
            working_hours_end: None,
            time_format: Some("24h".to_string()),
            zoom_link: None,
            teams_link: None,
            meet_link: None,
        };

        let updated = update_calendar_settings(&conn, &update).unwrap();
        assert_eq!(updated.default_view, "week");
        assert_eq!(updated.week_starts_on, 1);
        assert_eq!(updated.time_format, "24h");
    }

    #[test]
    fn test_meeting_templates() {
        let conn = open_test_db();

        let template =
            create_meeting_template(&conn, "Zoom", "https://zoom.us/j/{id}", "video").unwrap();
        assert_eq!(template.name, "Zoom");

        let all = list_meeting_templates(&conn).unwrap();
        assert_eq!(all.len(), 1);

        assert!(delete_meeting_template(&conn, template.id).unwrap());
        assert!(!delete_meeting_template(&conn, template.id).unwrap());

        let all = list_meeting_templates(&conn).unwrap();
        assert_eq!(all.len(), 0);
    }

    #[test]
    fn test_recurring_remote_event_expands() {
        let conn = open_test_db();
        let created = create_account(&conn, &account()).unwrap();

        let mut event = remote_event("rrule-1@x", "Weekly", "2026-03-02T09:00:00Z", "2026-03-02T09:30:00Z");
        event.recurrence_rule = Some("FREQ=WEEKLY;COUNT=4".to_string());
        replace_account_items(&conn, created.id, "caldav", "blue", &[event], &[]).unwrap();

        let listed = list_events(&conn, "2026-03-01T00:00:00Z", "2026-04-30T23:59:59Z").unwrap();
        assert_eq!(listed.len(), 4);
    }
}
