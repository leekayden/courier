//! Calendar sync orchestration.
//!
//! [`CalendarSyncManager`] is the long-lived service: it throttles syncs,
//! caches Google access tokens, and signs Google OAuth states. The actual
//! fetch-and-cache work happens in [`sync_account`], called from route
//! handlers right before serving cached data.
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hmac::{KeyInit, Mac, SimpleHmac};
use rand::RngCore;
use reqwest::Client;
use sha2::Sha256;

use crate::db::calendar::{AccountCredentials, RemoteEvent, RemoteTodo};

use super::{caldav, google};

type HmacSha256 = SimpleHmac<Sha256>;

/// Minimum interval between remote fetches for one account.
const SYNC_THROTTLE: Duration = Duration::from_secs(60);

/// OAuth states older than this are rejected.
const OAUTH_STATE_TTL_SECS: u64 = 600;

/// Google access tokens are cached until shortly before their expiry.
const TOKEN_EXPIRY_MARGIN_SECS: i64 = 60;

struct CachedToken {
    token: String,
    expires_at: Instant,
}

/// Long-lived per-process calendar service (see module docs).
pub struct CalendarSyncManager {
    last_sync: Mutex<HashMap<i64, Instant>>,
    tokens: Mutex<HashMap<i64, CachedToken>>,
    oauth_key: Vec<u8>,
}

impl CalendarSyncManager {
    /// Creates the manager, loading (or generating) the OAuth state signing
    /// key from `{data_dir}/calendar_oauth.key`.
    pub fn new(data_dir: &str) -> Result<Self, String> {
        let oauth_key = load_or_create_key(data_dir)?;
        Ok(Self {
            last_sync: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            oauth_key,
        })
    }

    /// Returns true when a remote fetch for this account is due, marking the
    /// attempt so bursts of list requests trigger at most one fetch.
    pub fn should_sync(&self, account_id: i64) -> bool {
        let mut last = self.last_sync.lock().expect("sync throttle poisoned");
        match last.get(&account_id) {
            Some(at) if at.elapsed() < SYNC_THROTTLE => false,
            _ => {
                last.insert(account_id, Instant::now());
                true
            }
        }
    }

    /// Drop the throttle and any cached OAuth token for this account so the
    /// next list request syncs fresh (used after account edits).
    pub fn invalidate(&self, account_id: i64) {
        self.last_sync
            .lock()
            .expect("sync throttle poisoned")
            .remove(&account_id);
        self.tokens
            .lock()
            .expect("token cache poisoned")
            .remove(&account_id);
    }

    pub fn cached_token(&self, account_id: i64) -> Option<String> {
        let tokens = self.tokens.lock().expect("token cache poisoned");
        let cached = tokens.get(&account_id)?;
        if cached.expires_at > Instant::now() {
            Some(cached.token.clone())
        } else {
            None
        }
    }

    pub fn store_token(&self, account_id: i64, token: String, expires_in_secs: i64) {
        let expires_at = Instant::now()
            + Duration::from_secs((expires_in_secs - TOKEN_EXPIRY_MARGIN_SECS).max(30) as u64);
        self.tokens
            .lock()
            .expect("token cache poisoned")
            .insert(account_id, CachedToken { token, expires_at });
    }

    /// Sign a Google OAuth state carrying the user hash. The session cookie is
    /// SameSite=Strict so it is absent on the cross-site callback; the signed
    /// state (10-minute TTL) identifies the user instead.
    pub fn sign_oauth_state(&self, user_hash: &str) -> String {
        let exp = now_secs() + OAUTH_STATE_TTL_SECS;
        let payload = format!("{exp}.{user_hash}");
        let sig = hex(&hmac_sha256(&self.oauth_key, payload.as_bytes()));
        format!("{payload}.{sig}")
    }

    /// Verify a signed OAuth state and return the user hash it was issued for.
    pub fn verify_oauth_state(&self, state: &str) -> Option<String> {
        let (exp, user_hash, sig) = {
            let mut parts = state.rsplitn(3, '.');
            let sig = parts.next()?;
            let user_hash = parts.next()?;
            let exp = parts.next()?;
            (exp, user_hash, sig)
        };
        let exp: u64 = exp.parse().ok()?;
        if now_secs() > exp {
            return None;
        }
        let expected = hex(&hmac_sha256(
            &self.oauth_key,
            format!("{exp}.{user_hash}").as_bytes(),
        ));
        if !eq_ignore_case(&expected, sig) {
            return None;
        }
        Some(user_hash.to_string())
    }
}

/// Fetch fresh data for one account from its provider. Returns the remote
/// events and todos; the caller replaces the account's cache with them.
pub async fn sync_account(
    http: &Client,
    manager: &CalendarSyncManager,
    oauth: Option<&google::GoogleOAuthConfig>,
    creds: &AccountCredentials,
    start: &str,
    end: &str,
) -> Result<(Vec<RemoteEvent>, Vec<RemoteTodo>), String> {
    match creds.provider.as_str() {
        "caldav" => sync_caldav(http, creds, start, end).await,
        "google" => {
            sync_google(http, manager, oauth, creds, start, end).await
        }
        other => Err(format!("Unknown calendar provider '{other}'")),
    }
}

async fn sync_caldav(
    http: &Client,
    creds: &AccountCredentials,
    start: &str,
    end: &str,
) -> Result<(Vec<RemoteEvent>, Vec<RemoteTodo>), String> {
    // Events come time-filtered; todos come unfiltered so undated ones (which
    // no time range could ever match) are included too.
    let event_resources = caldav::fetch_resources(
        http,
        &creds.url,
        &creds.username,
        &creds.secret,
        "VEVENT",
        Some((start, end)),
    )
    .await?;
    let todo_resources = caldav::fetch_resources(
        http,
        &creds.url,
        &creds.username,
        &creds.secret,
        "VTODO",
        None,
    )
    .await?;

    let mut events = Vec::new();
    let mut todos = Vec::new();
    let mut seen_hrefs = std::collections::HashSet::new();

    for resource in event_resources.into_iter().chain(todo_resources) {
        if !seen_hrefs.insert(resource.href.clone()) {
            // A mixed VEVENT+VTODO collection returns the same resource for
            // both reports; parse it once.
            continue;
        }
        let (mut resource_events, mut resource_todos) =
            super::ics::parse_ics_full(&resource.ics)
                .map_err(|e| format!("Failed to parse calendar data from {}: {e}", resource.href))?;
        for event in &mut resource_events {
            if event.uid.is_empty() {
                event.uid = resource.href.clone();
            }
            event.remote_href = resource.href.clone();
            event.remote_etag = resource.etag.clone();
        }
        for todo in &mut resource_todos {
            if todo.uid.is_empty() {
                todo.uid = resource.href.clone();
            }
            todo.remote_href = resource.href.clone();
            todo.remote_etag = resource.etag.clone();
        }
        events.append(&mut resource_events);
        todos.append(&mut resource_todos);
    }

    Ok((events, todos))
}

async fn sync_google(
    http: &Client,
    manager: &CalendarSyncManager,
    oauth: Option<&google::GoogleOAuthConfig>,
    creds: &AccountCredentials,
    start: &str,
    end: &str,
) -> Result<(Vec<RemoteEvent>, Vec<RemoteTodo>), String> {
    let Some(google) = oauth else {
        return Err(
            "Google calendar sync requires GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET on the server"
                .to_string(),
        );
    };

    let access_token = match manager.cached_token(creds.id) {
        Some(token) => token,
        None => {
            let (token, expires_in) =
                google::refresh_access_token(http, google, &creds.secret).await?;
            manager.store_token(creds.id, token.clone(), expires_in);
            token
        }
    };

    let calendar_id = if creds.url.is_empty() {
        "primary"
    } else {
        creds.url.as_str()
    };
    let events = google::fetch_events(http, &access_token, calendar_id, start, end).await?;
    let todos = google::fetch_todos(http, &access_token).await.unwrap_or_else(|e| {
        tracing::warn!(account = creds.id, "Google Tasks fetch failed: {e}");
        Vec::new()
    });

    Ok((events, todos))
}

fn load_or_create_key(data_dir: &str) -> Result<Vec<u8>, String> {
    let path = Path::new(data_dir).join("calendar_oauth.key");
    if path.exists() {
        let hex_str = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read calendar_oauth.key: {e}"))?;
        return decode_hex(hex_str.trim())
            .map_err(|e| format!("calendar_oauth.key is corrupt: {e}"));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create data directory {parent:?}: {e}"))?;
    }
    let mut secret = [0u8; 32];
    rand::rng().fill_bytes(&mut secret);
    fs::write(&path, hex(&secret))
        .map_err(|e| format!("Failed to write calendar_oauth.key: {e}"))?;
    tracing::info!("Generated new calendar OAuth signing key at {}", path.display());
    Ok(secret.to_vec())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac key accepts any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("odd-length hex".to_string());
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> CalendarSyncManager {
        let dir = tempfile::tempdir().unwrap();
        CalendarSyncManager::new(dir.path().to_str().unwrap()).unwrap()
    }

    #[test]
    fn test_sync_throttle() {
        let mgr = manager();
        assert!(mgr.should_sync(1), "first attempt syncs");
        assert!(!mgr.should_sync(1), "immediately after, throttled");
        mgr.invalidate(1);
        assert!(mgr.should_sync(1), "invalidate lifts the throttle");
        assert!(mgr.should_sync(2), "other accounts unaffected");
    }

    #[test]
    fn test_oauth_state_roundtrip_and_expiry() {
        let mgr = manager();
        let state = mgr.sign_oauth_state("abc123");
        assert_eq!(mgr.verify_oauth_state(&state).as_deref(), Some("abc123"));

        assert_eq!(mgr.verify_oauth_state("garbage"), None);
        assert_eq!(
            mgr.verify_oauth_state(&format!("{state}tampered")),
            None
        );

        // Expired state: forge one with a past expiry using the same key.
        let expired_payload = "1.abc123".to_string();
        let sig = hex(&hmac_sha256(
            &mgr.oauth_key,
            expired_payload.as_bytes(),
        ));
        let expired = format!("{expired_payload}.{sig}");
        assert_eq!(mgr.verify_oauth_state(&expired), None);
    }

    #[test]
    fn test_hex_roundtrip() {
        let bytes = vec![0x00, 0xff, 0x1a];
        assert_eq!(decode_hex(&hex(&bytes)).unwrap(), bytes);
    }
}
