use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

use crate::auth::session::SessionState;
use crate::calendar;
use crate::db;
use crate::db::calendar::{
    CreateCalendarAccount, UpdateCalendarAccount, UpdateCalendarSettings,
};
use crate::error::AppError;
use crate::routes::AppServices;

// ---------------------------------------------------------------------------
// Query / request types
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/calendar/events`.
#[derive(Deserialize)]
pub struct ListEventsParams {
    /// Start of the time range (ISO 8601).
    pub start: String,
    /// End of the time range (ISO 8601).
    pub end: String,
}

/// JSON body for `POST /api/calendar/meeting-templates`.
#[derive(Deserialize)]
pub struct CreateMeetingTemplateRequest {
    pub name: String,
    pub url_template: String,
    #[serde(default)]
    pub icon: String,
}

/// JSON body for `POST /api/calendar/caldav/discover`.
#[derive(Deserialize)]
pub struct CalDavDiscoverRequest {
    pub url: String,
    pub username: String,
    pub secret: String,
}

/// JSON body for the Google OAuth callback query string.
#[derive(Deserialize)]
pub struct GoogleCallbackParams {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// One calendar account as shown to the frontend (secrets redacted).
#[derive(Serialize)]
pub struct AccountSummary {
    pub id: i64,
    pub provider: String,
    pub name: String,
    pub color: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
    pub last_sync_error: Option<String>,
}

/// The calendar feature status.
#[derive(Serialize)]
pub struct CalendarStatusResponse {
    pub configured: bool,
    pub account_count: i64,
    pub enabled_account_count: i64,
    pub legacy_event_count: i64,
    /// Whether the server has Google OAuth credentials configured.
    pub google_oauth_enabled: bool,
}

fn internal_error(e: String) -> AppError {
    AppError::InternalError(format!("Database error: {e}"))
}

// ---------------------------------------------------------------------------
// Status + event handlers
// ---------------------------------------------------------------------------

/// `GET /api/calendar/status`
///
/// Whether the calendar is configured (i.e. enabled) for this user.
pub async fn get_status(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
) -> Result<Response, AppError> {
    let status = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, |conn| {
        db::calendar::get_calendar_status(conn)
    })
    .await
    .map_err(internal_error)?;

    Ok(Json(CalendarStatusResponse {
        configured: status.configured,
        account_count: status.account_count,
        enabled_account_count: status.enabled_account_count,
        legacy_event_count: status.legacy_event_count,
        google_oauth_enabled: calendar::google::oauth_config(&svc.config).is_some(),
    })
    .into_response())
}

/// `GET /api/calendar/events?start=...&end=...`
///
/// List events and todos within a time range, syncing any enabled provider
/// accounts first (throttled to one fetch per account per minute).
///
/// The calendar is read-only: there are no create/update/delete endpoints.
pub async fn list_events(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
    Query(params): Query<ListEventsParams>,
) -> Result<Response, AppError> {
    let accounts = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, |conn| {
        db::calendar::list_enabled_credentials(conn)
    })
    .await
    .map_err(internal_error)?;

    let oauth = calendar::google::oauth_config(&svc.config);
    let mut sync_errors: HashMap<String, String> = HashMap::new();

    for creds in &accounts {
        if !svc.calendar_sync.should_sync(creds.id) {
            continue;
        }
        let account_id = creds.id;

        match calendar::sync::sync_account(
            &svc.http_client,
            &svc.calendar_sync,
            oauth.as_ref(),
            creds,
            &params.start,
            &params.end,
        )
        .await
        {
            Ok((events, todos)) => {
                let provider = creds.provider.clone();
                let color = creds.color.clone();
                if let Err(e) = db::pool::with_user_db(
                    &svc.db_pool_manager,
                    &session.user_hash,
                    move |conn| {
                        db::calendar::replace_account_items(
                            conn, account_id, &provider, &color, &events, &todos,
                        )?;
                        db::calendar::set_account_sync_state(conn, account_id, None)
                    },
                )
                .await
                {
                    sync_errors.insert(account_id.to_string(), e);
                }
            }
            Err(e) => {
                tracing::warn!(account = account_id, "Calendar sync failed: {e}");
                sync_errors.insert(account_id.to_string(), e.clone());
                let _ = db::pool::with_user_db(
                    &svc.db_pool_manager,
                    &session.user_hash,
                    move |conn| {
                        db::calendar::set_account_sync_state(conn, account_id, Some(&e))
                    },
                )
                .await;
            }
        }
    }

    let (events, todos, summaries, status) =
        db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, move |conn| {
            let events = db::calendar::list_events(conn, &params.start, &params.end)?;
            let todos = db::calendar::list_todos(conn)?;
            let summaries = db::calendar::list_accounts(conn)?
                .into_iter()
                .map(|a| AccountSummary {
                    id: a.id,
                    provider: a.provider,
                    name: a.name,
                    color: a.color,
                    enabled: a.enabled,
                    last_synced_at: a.last_synced_at,
                    last_sync_error: a.last_sync_error,
                })
                .collect::<Vec<_>>();
            let status = db::calendar::get_calendar_status(conn)?;
            Ok((events, todos, summaries, status))
        })
        .await
        .map_err(internal_error)?;

    let configured = !accounts.is_empty() || status.legacy_event_count > 0;

    Ok(Json(serde_json::json!({
        "configured": configured,
        "events": events,
        "todos": todos,
        "accounts": summaries,
        "sync_errors": sync_errors,
    }))
    .into_response())
}

/// `GET /api/calendar/events/{id}`
///
/// Get a single event or todo by ID.
pub async fn get_event(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let event = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, {
        let id = id.clone();
        move |conn| db::calendar::get_event(conn, &id)
    })
    .await
    .map_err(internal_error)?;

    match event {
        Some(e) => Ok(Json(e).into_response()),
        None => Err(AppError::NotFound(format!("Event '{id}' not found"))),
    }
}

// ---------------------------------------------------------------------------
// Account management
// ---------------------------------------------------------------------------

/// `GET /api/calendar/accounts`
pub async fn list_accounts(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
) -> Result<Response, AppError> {
    let accounts = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, |conn| {
        db::calendar::list_accounts(conn)
    })
    .await
    .map_err(internal_error)?;

    Ok(Json(serde_json::json!({ "accounts": accounts })).into_response())
}

/// `POST /api/calendar/accounts`
///
/// Configure a new calendar provider account. This is what enables the
/// previously-disabled calendar.
pub async fn create_account(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
    Json(body): Json<CreateCalendarAccount>,
) -> Result<Response, AppError> {
    if body.provider != "caldav" && body.provider != "google" {
        return Err(AppError::BadRequest(
            "Provider must be 'caldav' or 'google'".to_string(),
        ));
    }
    if body.provider == "caldav" {
        if body.url.trim().is_empty() {
            return Err(AppError::BadRequest("CalDAV URL is required".to_string()));
        }
        if body.username.trim().is_empty() {
            return Err(AppError::BadRequest(
                "CalDAV username is required".to_string(),
            ));
        }
        if body.secret.trim().is_empty() {
            return Err(AppError::BadRequest(
                "CalDAV password is required".to_string(),
            ));
        }
    }

    let account = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, move |conn| {
        db::calendar::create_account(conn, &body)
    })
    .await
    .map_err(internal_error)?;

    svc.calendar_sync.invalidate(account.id);

    Ok((StatusCode::CREATED, Json(account)).into_response())
}

/// `PUT /api/calendar/accounts/{id}`
pub async fn update_account(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
    Path(id): Path<i64>,
    Json(body): Json<UpdateCalendarAccount>,
) -> Result<Response, AppError> {
    let account = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, move |conn| {
        db::calendar::update_account(conn, id, &body)
    })
    .await
    .map_err(internal_error)?;

    match account {
        Some(account) => {
            svc.calendar_sync.invalidate(id);
            Ok(Json(account).into_response())
        }
        None => Err(AppError::NotFound(format!("Account '{id}' not found"))),
    }
}

/// `DELETE /api/calendar/accounts/{id}`
///
/// Removes the account and its cached items. With no accounts left the
/// calendar becomes disabled again.
pub async fn delete_account(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let deleted = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, move |conn| {
        db::calendar::delete_account(conn, id)
    })
    .await
    .map_err(internal_error)?;

    if deleted {
        svc.calendar_sync.invalidate(id);
        Ok(Json(serde_json::json!({ "status": "deleted" })).into_response())
    } else {
        Err(AppError::NotFound(format!("Account '{id}' not found")))
    }
}

/// `POST /api/calendar/caldav/discover`
///
/// Probe a CalDAV URL with the given credentials and list the calendar
/// collections found there (used by the settings UI to pick a calendar).
pub async fn caldav_discover(
    svc: AppServices,
    Json(body): Json<CalDavDiscoverRequest>,
) -> Result<Response, AppError> {
    if body.url.trim().is_empty() {
        return Err(AppError::BadRequest("URL is required".to_string()));
    }

    let collections = calendar::caldav::discover_calendars(
        &svc.http_client,
        body.url.trim(),
        body.username.trim(),
        &body.secret,
    )
    .await
    .map_err(AppError::BadRequest)?;

    Ok(Json(serde_json::json!({ "collections": collections })).into_response())
}

// ---------------------------------------------------------------------------
// Google OAuth
// ---------------------------------------------------------------------------

/// `GET /api/calendar/google/status`
///
/// Whether the server has Google OAuth credentials configured.
pub async fn google_status(svc: AppServices) -> Result<Response, AppError> {
    Ok(Json(serde_json::json!({
        "google_oauth_enabled": calendar::google::oauth_config(&svc.config).is_some(),
    }))
    .into_response())
}

/// `GET /api/calendar/google/auth-url`
///
/// Returns the Google consent URL for the signed-in user. The redirect URI is
/// derived from the request Host (respecting X-Forwarded-Proto and BASE_PATH)
/// and must be registered as an authorized redirect URI in the Google Cloud
/// console.
pub async fn google_auth_url(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let Some(google) = calendar::google::oauth_config(&svc.config) else {
        return Err(AppError::InternalError(
            "Google calendar integration is not configured on this server (set GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET)".to_string(),
        ));
    };

    let state = svc.calendar_sync.sign_oauth_state(&session.user_hash);
    let redirect_uri = google_redirect_uri(&svc.config, &headers);
    let url = calendar::google::build_auth_url(&google, &redirect_uri, &state);

    Ok(Json(serde_json::json!({ "url": url, "redirect_uri": redirect_uri })).into_response())
}

/// `GET /api/calendar/google/callback?code=...&state=...`
///
/// OAuth redirect target. Lives outside the auth_guard because the session
/// cookie is SameSite=Strict and absent on this cross-site navigation; the
/// signed state identifies the user instead. Exchanges the code, stores the
/// refresh token, and redirects back to the app.
pub async fn google_callback(
    svc: AppServices,
    headers: HeaderMap,
    Query(params): Query<GoogleCallbackParams>,
) -> Response {
    if let Some(error) = params.error.as_deref() {
        return redirect_to_app(&svc.config, format!("calendar_error={}", urlencode(error)));
    }
    let (Some(code), Some(state)) = (params.code.as_deref(), params.state.as_deref()) else {
        return redirect_to_app(&svc.config, "calendar_error=missing_code_or_state".to_string());
    };

    let Some(user_hash) = svc.calendar_sync.verify_oauth_state(state) else {
        return redirect_to_app(&svc.config, "calendar_error=invalid_or_expired_state".to_string());
    };

    let Some(google) = calendar::google::oauth_config(&svc.config) else {
        return redirect_to_app(&svc.config, "calendar_error=google_not_configured".to_string());
    };

    let redirect_uri = google_redirect_uri(&svc.config, &headers);
    let exchange =
        calendar::google::exchange_code(&svc.http_client, &google, code, &redirect_uri).await;

    let exchange = match exchange {
        Ok(exchange) => exchange,
        Err(e) => {
            tracing::warn!("Google OAuth exchange failed: {e}");
            return redirect_to_app(&svc.config, "calendar_error=token_exchange_failed".to_string());
        }
    };

    // Reuse an existing Google account for the same email; otherwise create one.
    let result = db::pool::with_user_db(&svc.db_pool_manager, &user_hash, move |conn| {
        let accounts = db::calendar::list_accounts(conn)?;
        let existing = accounts
            .iter()
            .find(|a| a.provider == "google" && a.username == exchange.email);
        let account = if let Some(existing) = existing {
            db::calendar::update_account(
                conn,
                existing.id,
                &UpdateCalendarAccount {
                    name: None,
                    color: None,
                    url: None,
                    username: None,
                    secret: Some(exchange.refresh_token.clone()),
                    enabled: Some(true),
                },
            )?
            .ok_or("failed to update google account")?
        } else {
            db::calendar::create_account(
                conn,
                &CreateCalendarAccount {
                    provider: "google".to_string(),
                    name: exchange.email.clone(),
                    color: "blue".to_string(),
                    url: "primary".to_string(),
                    username: exchange.email.clone(),
                    secret: exchange.refresh_token,
                },
            )?
        };
        Ok(account.id)
    })
    .await;

    match result {
        Ok(account_id) => {
            svc.calendar_sync.invalidate(account_id);
            redirect_to_app(&svc.config, "calendar=connected".to_string())
        }
        Err(e) => {
            tracing::warn!("Failed to store Google account: {e}");
            redirect_to_app(&svc.config, "calendar_error=storage_failed".to_string())
        }
    }
}

/// Compute the Google OAuth redirect URI from request headers.
///
/// Behind a reverse proxy the Host header carries the public host and
/// X-Forwarded-Proto the public scheme; BASE_PATH is honored the same way the
/// router nests the API.
fn google_redirect_uri(config: &Arc<crate::config::AppConfig>, headers: &HeaderMap) -> String {
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .unwrap_or("http")
        .trim();
    let base = match config.base_path.as_deref() {
        Some(bp) if !bp.is_empty() => bp.trim_matches('/'),
        _ => "",
    };
    if base.is_empty() {
        format!("{proto}://{host}/api/calendar/google/callback")
    } else {
        format!("{proto}://{host}/{base}/api/calendar/google/callback")
    }
}

fn redirect_to_app(config: &Arc<crate::config::AppConfig>, query: String) -> Response {
    let base = match config.base_path.as_deref() {
        Some(bp) if !bp.is_empty() => bp.trim_matches('/'),
        _ => "",
    };
    let target = if base.is_empty() {
        format!("/mail?{query}")
    } else {
        format!("/{base}/mail?{query}")
    };
    Redirect::to(&target).into_response()
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

// ---------------------------------------------------------------------------
// Settings handlers
// ---------------------------------------------------------------------------

/// `GET /api/calendar/settings`
///
/// Get calendar settings.
pub async fn get_calendar_settings(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
) -> Result<Response, AppError> {
    let settings = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, |conn| {
        db::calendar::get_calendar_settings(conn)
    })
    .await
    .map_err(internal_error)?;

    Ok(Json(settings).into_response())
}

/// `PUT /api/calendar/settings`
///
/// Update calendar settings.
pub async fn update_calendar_settings(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
    Json(body): Json<UpdateCalendarSettings>,
) -> Result<Response, AppError> {
    let settings = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, move |conn| {
        db::calendar::update_calendar_settings(conn, &body)
    })
    .await
    .map_err(internal_error)?;

    Ok(Json(settings).into_response())
}

// ---------------------------------------------------------------------------
// Meeting template handlers
// ---------------------------------------------------------------------------

/// `GET /api/calendar/meeting-templates`
///
/// List all meeting templates.
pub async fn list_meeting_templates(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
) -> Result<Response, AppError> {
    let templates = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, |conn| {
        db::calendar::list_meeting_templates(conn)
    })
    .await
    .map_err(internal_error)?;

    Ok(Json(serde_json::json!({ "templates": templates })).into_response())
}

/// `POST /api/calendar/meeting-templates`
///
/// Create a new meeting template.
pub async fn create_meeting_template(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
    Json(body): Json<CreateMeetingTemplateRequest>,
) -> Result<Response, AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }
    if body.url_template.trim().is_empty() {
        return Err(AppError::BadRequest(
            "URL template is required".to_string(),
        ));
    }

    let template = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, move |conn| {
        db::calendar::create_meeting_template(conn, &body.name, &body.url_template, &body.icon)
    })
    .await
    .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(template)).into_response())
}

/// `DELETE /api/calendar/meeting-templates/{id}`
///
/// Delete a meeting template.
pub async fn delete_meeting_template(
    Extension(session): Extension<SessionState>,
    svc: AppServices,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let deleted = db::pool::with_user_db(&svc.db_pool_manager, &session.user_hash, move |conn| {
        db::calendar::delete_meeting_template(conn, id)
    })
    .await
    .map_err(internal_error)?;

    if deleted {
        Ok(Json(serde_json::json!({ "status": "deleted" })).into_response())
    } else {
        Err(AppError::NotFound(format!(
            "Meeting template '{id}' not found"
        )))
    }
}
