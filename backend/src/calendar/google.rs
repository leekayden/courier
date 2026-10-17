//! Read-only Google Calendar provider.
//!
//! Uses the Calendar API v3 for events and the Tasks API for todo-style
//! items. The server needs a Google OAuth client (GOOGLE_CLIENT_ID /
//! GOOGLE_CLIENT_SECRET); users authorize with the authorization-code flow
//! and the app stores only a refresh token scoped to readonly access.
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;

use crate::config::AppConfig;
use crate::db::calendar::{RemoteEvent, RemoteTodo};

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const USERINFO_ENDPOINT: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const CALENDAR_API: &str = "https://www.googleapis.com/calendar/v3";
const TASKS_API: &str = "https://tasks.googleapis.com/tasks/v1";

/// Readonly scopes: calendar events, tasks, and the user's email so the
/// account can be labelled.
pub const OAUTH_SCOPES: &str = "https://www.googleapis.com/auth/calendar.readonly \
     https://www.googleapis.com/auth/tasks.readonly openid email";

/// Server-side OAuth client credentials, when configured.
#[derive(Debug, Clone)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
}

pub fn oauth_config(config: &AppConfig) -> Option<GoogleOAuthConfig> {
    let client_id = config.google_client_id.as_deref()?.trim();
    let client_secret = config.google_client_secret.as_deref()?.trim();
    if client_id.is_empty() || client_secret.is_empty() {
        return None;
    }
    Some(GoogleOAuthConfig {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
    })
}

/// Build the consent-screen URL the user is redirected to.
pub fn build_auth_url(google: &GoogleOAuthConfig, redirect_uri: &str, state: &str) -> String {
    let params = [
        ("client_id", google.client_id.as_str()),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", OAUTH_SCOPES),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("include_granted_scopes", "false"),
        ("state", state),
    ];
    let query: String = params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{AUTH_ENDPOINT}?{query}")
}

fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Result of exchanging the authorization code: the stored refresh token and
/// the Google account email.
pub struct ExchangeResult {
    pub refresh_token: String,
    pub email: String,
}

pub async fn exchange_code(
    http: &Client,
    google: &GoogleOAuthConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<ExchangeResult, String> {
    let params = [
        ("code", code),
        ("client_id", google.client_id.as_str()),
        ("client_secret", google.client_secret.as_str()),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    let response = http
        .post(TOKEN_ENDPOINT)
        .timeout(Duration::from_secs(20))
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Google token request failed: {e}"))?;

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Google token response was not JSON: {e}"))?;
    let refresh_token = body["refresh_token"]
        .as_str()
        .ok_or_else(|| format!("Google did not return a refresh token: {body}"))?
        .to_string();

    let email = fetch_userinfo_email(http, body["access_token"].as_str().unwrap_or_default())
        .await
        .unwrap_or_default();

    Ok(ExchangeResult {
        refresh_token,
        email,
    })
}

async fn fetch_userinfo_email(http: &Client, access_token: &str) -> Result<String, String> {
    if access_token.is_empty() {
        return Err("no access token".to_string());
    }
    let response = http
        .get(USERINFO_ENDPOINT)
        .timeout(Duration::from_secs(15))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Google userinfo request failed: {e}"))?;
    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Google userinfo response was not JSON: {e}"))?;
    Ok(body["email"].as_str().unwrap_or_default().to_string())
}

/// Exchange a refresh token for a fresh access token.
pub async fn refresh_access_token(
    http: &Client,
    google: &GoogleOAuthConfig,
    refresh_token: &str,
) -> Result<(String, i64), String> {
    let params = [
        ("refresh_token", refresh_token),
        ("client_id", google.client_id.as_str()),
        ("client_secret", google.client_secret.as_str()),
        ("grant_type", "refresh_token"),
    ];
    let response = http
        .post(TOKEN_ENDPOINT)
        .timeout(Duration::from_secs(20))
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Google token refresh failed: {e}"))?;

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Google token refresh response was not JSON: {e}"))?;
    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(|| format!("Google token refresh failed: {body}"))?
        .to_string();
    let expires_in = body["expires_in"].as_i64().unwrap_or(3600);

    Ok((access_token, expires_in))
}

/// Fetch events in [start, end] from one Google calendar and map them to the
/// local cache format. Recurring events arrive already expanded
/// (`singleEvents=true`).
pub async fn fetch_events(
    http: &Client,
    access_token: &str,
    calendar_id: &str,
    start: &str,
    end: &str,
) -> Result<Vec<RemoteEvent>, String> {
    let url = format!(
        "{CALENDAR_API}/calendars/{}/events?singleEvents=true&orderBy=startTime&maxResults=2500&timeMin={}&timeMax={}",
        urlencode(calendar_id),
        urlencode(&to_rfc3339(start)),
        urlencode(&to_rfc3339(end)),
    );
    let response = http
        .get(&url)
        .timeout(Duration::from_secs(20))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Google Calendar request failed: {e}"))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Google Calendar response was not JSON: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "Google Calendar API returned {status}: {body}"
        ));
    }

    let mut events = Vec::new();
    for item in body["items"].as_array().cloned().unwrap_or_default() {
        if item["status"].as_str() == Some("cancelled") {
            continue;
        }
        let Some(event) = map_google_event(&item) else {
            continue;
        };
        events.push(event);
    }
    Ok(events)
}

fn map_google_event(item: &Value) -> Option<RemoteEvent> {
    let start_raw = item["start"]["dateTime"]
        .as_str()
        .map(str::to_string)
        .or_else(|| item["start"]["date"].as_str().map(str::to_string))?;
    let end_raw = item["end"]["dateTime"]
        .as_str()
        .map(str::to_string)
        .or_else(|| item["end"]["date"].as_str().map(str::to_string))?;

    let all_day = item["start"]["date"].is_string();
    let start_time = normalize_google_datetime(&start_raw);
    let end_time = normalize_google_datetime(&end_raw);

    let attendees: Vec<String> = item["attendees"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| a["email"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let attendees_json = serde_json::to_string(&attendees).unwrap_or_else(|_| "[]".to_string());

    let meeting_url = item["hangoutLink"]
        .as_str()
        .or_else(|| {
            item["conferenceData"]["entryPoints"]
                .as_array()
                .and_then(|eps| {
                    eps.iter()
                        .find(|ep| ep["entryPointType"].as_str() == Some("video"))
                        .and_then(|ep| ep["uri"].as_str())
                })
        })
        .map(str::to_string);

    Some(RemoteEvent {
        uid: item["id"].as_str().unwrap_or_default().to_string(),
        title: item["summary"]
            .as_str()
            .unwrap_or("(No title)")
            .to_string(),
        description: item["description"].as_str().unwrap_or_default().to_string(),
        location: item["location"].as_str().unwrap_or_default().to_string(),
        start_time,
        end_time,
        all_day,
        // singleEvents=true returns expanded occurrences, no RRULE needed.
        recurrence_rule: None,
        attendees: attendees_json,
        organizer: item["organizer"]["email"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        status: item["status"]
            .as_str()
            .unwrap_or("confirmed")
            .to_lowercase(),
        meeting_url,
        remote_href: item["htmlLink"].as_str().unwrap_or_default().to_string(),
        remote_etag: item["etag"].as_str().map(str::to_string),
    })
}

/// Fetch the user's task lists and their open/completed tasks (the Google
/// analogue of VTODOs).
pub async fn fetch_todos(http: &Client, access_token: &str) -> Result<Vec<RemoteTodo>, String> {
    let lists_url = format!("{TASKS_API}/users/@me/lists?maxResults=100");
    let response = http
        .get(&lists_url)
        .timeout(Duration::from_secs(20))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Google Tasks request failed: {e}"))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Google Tasks response was not JSON: {e}"))?;
    if !status.is_success() {
        // Tasks may be unavailable for Workspace accounts without Tasks
        // enabled; tolerate the failure and surface only events.
        return Err(format!("Google Tasks API returned {status}: {body}"));
    }

    let mut todos = Vec::new();
    for list in body["items"].as_array().cloned().unwrap_or_default() {
        let Some(list_id) = list["id"].as_str() else {
            continue;
        };
        let tasks_url = format!(
            "{TASKS_API}/lists/{}/tasks?maxResults=500&showCompleted=true&showHidden=false",
            urlencode(list_id)
        );
        let response = http
            .get(&tasks_url)
            .timeout(Duration::from_secs(20))
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| format!("Google Tasks request failed: {e}"))?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("Google Tasks response was not JSON: {e}"))?;
        if !status.is_success() {
            continue;
        }

        for item in body["items"].as_array().cloned().unwrap_or_default() {
            let completed = item["completed"].is_object();
            todos.push(RemoteTodo {
                uid: item["id"].as_str().unwrap_or_default().to_string(),
                title: item["title"]
                    .as_str()
                    .unwrap_or("(No title)")
                    .to_string(),
                description: item["notes"].as_str().unwrap_or_default().to_string(),
                start_time: String::new(),
                due_time: item["due"]
                    .as_str()
                    .map(normalize_google_datetime)
                    .unwrap_or_default(),
                status: if completed {
                    "completed".to_string()
                } else {
                    item["status"]
                        .as_str()
                        .unwrap_or("needsAction")
                        .replace("needsAction", "needs-action")
                },
                completed,
                completed_at: item["completed"]["dateTime"]
                    .as_str()
                    .map(normalize_google_datetime),
                percent_complete: None,
                priority: None,
                url: item["selfLink"].as_str().map(str::to_string),
                remote_href: item["selfLink"].as_str().unwrap_or_default().to_string(),
                remote_etag: item["etag"].as_str().map(str::to_string),
            });
        }
    }
    Ok(todos)
}

/// Normalize a Google datetime (`2026-03-01T09:00:00+01:00` or
/// `2026-03-01T09:00:00Z`) to UTC `...Z` so range filtering in SQLite stays
/// lexicographically consistent. Date-only values pass through unchanged.
fn normalize_google_datetime(value: &str) -> String {
    let value = value.trim();
    if value.len() == 10 && !value.contains('T') {
        return value.to_string();
    }
    match value.parse::<DateTime<Utc>>() {
        Ok(dt) => dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        Err(_) => match DateTime::parse_from_rfc3339(value) {
            Ok(dt) => dt.with_timezone(&Utc).format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            Err(_) => value.to_string(),
        },
    }
}

/// Range bounds arrive in the API's `...Z` ISO format; pass them through (or
/// coerce date-only values to midnight) for the API's RFC 3339 parameters.
fn to_rfc3339(value: &str) -> String {
    value.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_google_datetime() {
        assert_eq!(
            normalize_google_datetime("2026-03-01T09:00:00+01:00"),
            "2026-03-01T08:00:00Z"
        );
        assert_eq!(
            normalize_google_datetime("2026-03-01T09:00:00Z"),
            "2026-03-01T09:00:00Z"
        );
        assert_eq!(normalize_google_datetime("2026-03-01"), "2026-03-01");
    }

    #[test]
    fn test_map_google_event() {
        let item: Value = serde_json::json!({
            "id": "abc123",
            "summary": "Review",
            "description": "notes",
            "location": "Room 2",
            "status": "confirmed",
            "htmlLink": "https://calendar.google.com/...",
            "etag": "\"e1\"",
            "start": {"dateTime": "2026-03-02T09:00:00+02:00"},
            "end": {"dateTime": "2026-03-02T10:00:00+02:00"},
            "attendees": [{"email": "a@x.com"}, {"email": "b@x.com"}],
            "organizer": {"email": "org@x.com"},
            "hangoutLink": "https://meet.google.com/xyz"
        });
        let event = map_google_event(&item).unwrap();
        assert_eq!(event.uid, "abc123");
        assert_eq!(event.title, "Review");
        assert_eq!(event.start_time, "2026-03-02T07:00:00Z");
        assert_eq!(event.end_time, "2026-03-02T08:00:00Z");
        assert!(!event.all_day);
        assert_eq!(event.organizer, "org@x.com");
        assert_eq!(event.meeting_url.as_deref(), Some("https://meet.google.com/xyz"));
        let attendees: Vec<String> = serde_json::from_str(&event.attendees).unwrap();
        assert_eq!(attendees.len(), 2);
    }

    #[test]
    fn test_map_google_all_day_event() {
        let item: Value = serde_json::json!({
            "id": "allday",
            "summary": "Offsite",
            "status": "confirmed",
            "start": {"date": "2026-03-05"},
            "end": {"date": "2026-03-06"}
        });
        let event = map_google_event(&item).unwrap();
        assert!(event.all_day);
        assert_eq!(event.start_time, "2026-03-05");
        assert_eq!(event.end_time, "2026-03-06");
    }

    #[test]
    fn test_build_auth_url_encodes_params() {
        let google = GoogleOAuthConfig {
            client_id: "my-client".to_string(),
            client_secret: "secret".to_string(),
        };
        let url = build_auth_url(&google, "https://mail.example.com/api/calendar/google/callback", "st.ate-1");
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?client_id=my-client&"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fmail.example.com%2Fapi%2Fcalendar%2Fgoogle%2Fcallback"));
        assert!(url.contains("state=st.ate-1"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcalendar.readonly"));
    }
}
