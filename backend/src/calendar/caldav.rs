//! Minimal read-only CalDAV client.
//!
//! Supports the two operations the calendar needs:
//! - `PROPFIND` (Depth 1) to discover calendar collections under a URL
//! - `REPORT calendar-query` to fetch VEVENTs (optionally time-filtered) and
//!   VTODOs as raw iCalendar data
//!
//! Auth is HTTP Basic over the shared reqwest client (works with Baikal,
//! Nextcloud, Radicale, Fastmail, etc.).
use std::time::Duration;

use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use url::Url;

/// A calendar collection discovered via PROPFIND.
#[derive(Debug, Clone, Serialize)]
pub struct DavCollection {
    pub href: String,
    pub displayname: String,
    /// Calendar components the collection supports, e.g. ["VEVENT", "VTODO"].
    pub components: Vec<String>,
}

/// A single resource returned by a calendar-query REPORT.
#[derive(Debug, Clone)]
pub struct DavResource {
    pub href: String,
    pub etag: Option<String>,
    /// Raw iCalendar body.
    pub ics: String,
}

/// Intermediate per-`<D:response>` data collected while parsing multistatus.
#[derive(Debug, Default)]
struct DavItem {
    href: String,
    displayname: Option<String>,
    is_calendar: bool,
    components: Vec<String>,
    etag: Option<String>,
    calendar_data: Option<String>,
}

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <d:displayname/>
    <d:resourcetype/>
    <cal:supported-calendar-component-set/>
  </d:prop>
</d:propfind>"#;

/// Convert an ISO datetime (`2026-03-01T00:00:00Z`) or date (`2026-03-01`)
/// into the compact iCalendar format time-range filters expect.
fn compact_ical_datetime(iso: &str) -> String {
    let iso = iso.trim();
    if iso.len() == 10 && !iso.contains('T') {
        format!("{}T000000Z", iso.replace('-', ""))
    } else {
        iso.replace(['-', ':'], "")
    }
}

/// List calendar collections reachable at `url` (Depth 1).
///
/// `url` may be a calendar home (e.g. `.../dav.php/calendars/kayden/`) or a
/// single collection (e.g. `.../dav.php/calendars/kayden/default/`). Only
/// entries whose resourcetype includes the CalDAV `calendar` type are
/// returned; a PROPFIND on a collection itself therefore yields that one
/// collection.
pub async fn discover_calendars(
    client: &Client,
    url: &str,
    username: &str,
    password: &str,
) -> Result<Vec<DavCollection>, String> {
    let response =
        request_with_basic_auth(client, dav_method("PROPFIND"), url, username, password)
            .await?
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(PROPFIND_BODY)
            .send()
            .await
            .map_err(|e| format!("CalDAV request failed: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    check_dav_status(status, &body)?;

    let items = parse_multistatus(&body)?;
    let base = request_url(url)?;
    let mut collections = Vec::new();
    for item in items {
        if !item.is_calendar {
            continue;
        }
        collections.push(DavCollection {
            href: resolve_href(&base, &item.href)?,
            displayname: item.displayname.unwrap_or_default(),
            components: item.components,
        });
    }
    Ok(collections)
}

/// Run a calendar-query REPORT and return the matching resources.
///
/// `component` selects VEVENT or VTODO; when `time_range` is given the server
/// filters VEVENTs (or dated todos) to the window. Todos are normally fetched
/// without a range so undated ones are included.
pub async fn fetch_resources(
    client: &Client,
    collection_url: &str,
    username: &str,
    password: &str,
    component: &str,
    time_range: Option<(&str, &str)>,
) -> Result<Vec<DavResource>, String> {
    let time_xml = match time_range {
        Some((start, end)) => format!(
            "<C:time-range start=\"{}\" end=\"{}\"/>",
            compact_ical_datetime(start),
            compact_ical_datetime(end),
        ),
        None => String::new(),
    };
    let report_body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:getetag/>
    <C:calendar-data/>
  </D:prop>
  <C:filter>
    <C:comp-filter name="VCALENDAR">
      <C:comp-filter name="{component}">{time_xml}</C:comp-filter>
    </C:comp-filter>
  </C:filter>
</C:calendar-query>"#
    );

    let response =
        request_with_basic_auth(client, dav_method("REPORT"), collection_url, username, password)
            .await?
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(report_body)
            .send()
            .await
            .map_err(|e| format!("CalDAV request failed: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    check_dav_status(status, &body)?;

    let base = request_url(collection_url)?;
    let items = parse_multistatus(&body)?;
    let mut resources = Vec::new();
    for item in items {
        let Some(ics) = item.calendar_data else {
            continue;
        };
        if ics.trim().is_empty() {
            continue;
        }
        resources.push(DavResource {
            href: resolve_href(&base, &item.href)?,
            etag: item.etag,
            ics,
        });
    }
    Ok(resources)
}

/// Build a WebDAV method (PROPFIND, REPORT) that the `http` crate has no
/// constant for. These strings are statically valid.
fn dav_method(name: &str) -> Method {
    Method::from_bytes(name.as_bytes()).expect("WebDAV method name is valid")
}

async fn request_with_basic_auth(
    client: &Client,
    method: Method,
    url: &str,
    username: &str,
    password: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let parsed = request_url(url)?;
    Ok(client
        .request(method, parsed)
        .timeout(Duration::from_secs(30))
        .basic_auth(username, Some(password)))
}fn request_url(url: &str) -> Result<Url, String> {
    Url::parse(url).map_err(|e| format!("Invalid CalDAV URL '{url}': {e}"))
}

fn check_dav_status(status: StatusCode, body: &str) -> Result<(), String> {
    if status.is_success() {
        return Ok(());
    }
    let hint = match status {
        StatusCode::UNAUTHORIZED => " — check the username and password (some servers require an app-specific password)".to_string(),
        StatusCode::FORBIDDEN => " — the server denied access to this calendar".to_string(),
        StatusCode::NOT_FOUND => " — the URL was not found on the server".to_string(),
        _ => String::new(),
    };
    let snippet: String = body.chars().take(200).collect();
    Err(format!(
        "CalDAV server returned {status}{hint}: {snippet}"
    ))
}

/// Resolve a (possibly relative or percent-encoded) DAV href against the
/// request URL.
fn resolve_href(base: &Url, href: &str) -> Result<String, String> {
    let trimmed = href.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    Url::options()
        .base_url(Some(base))
        .parse(trimmed)
        .map(|u| u.to_string())
        .map_err(|e| format!("Failed to resolve DAV href '{trimmed}': {e}"))
}

fn xml_local_name(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or("")
}

/// Parse a WebDAV 207 multistatus body.
fn parse_multistatus(xml: &str) -> Result<Vec<DavItem>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut items: Vec<DavItem> = Vec::new();
    let mut cur: Option<DavItem> = None;
    // Stack of local names of open elements.
    let mut path: Vec<String> = Vec::new();
    // Field name ("href" | "displayname" | "etag" | "calendar-data") whose
    // text content we are accumulating, if any.
    let mut capture: Option<String> = None;
    let mut text = String::new();

    let inside = |path: &[String], element: &str| path.iter().any(|p| p == element);

    loop {
        match reader.read_event().map_err(|e| format!("Bad XML from CalDAV server: {e}"))? {
            Event::Start(e) => {
                let name = xml_local_name(e.local_name().as_ref()).to_string();

                match name.as_str() {
                    "response" => cur = Some(DavItem::default()),
                    "displayname" | "href" | "getetag" | "calendar-data" if cur.is_some() => {
                        capture = Some(name.clone());
                        text.clear();
                    }
                    "calendar" if inside(&path, "resourcetype") => {
                        if let Some(item) = cur.as_mut() {
                            item.is_calendar = true;
                        }
                    }
                    "comp" if inside(&path, "supported-calendar-component-set") => {
                        if let Ok(Some(attr)) = e.try_get_attribute("name")
                            && let Ok(value) = String::from_utf8(attr.value.to_vec())
                            && let Some(item) = cur.as_mut()
                        {
                            item.components.push(value);
                        }
                    }
                    _ => {}
                }
                path.push(name);
            }
            Event::Empty(e) => {
                let name = xml_local_name(e.local_name().as_ref()).to_string();
                match name.as_str() {
                    "calendar" if inside(&path, "resourcetype") => {
                        if let Some(item) = cur.as_mut() {
                            item.is_calendar = true;
                        }
                    }
                    "comp" if inside(&path, "supported-calendar-component-set") => {
                        if let Ok(Some(attr)) = e.try_get_attribute("name")
                            && let Ok(value) = String::from_utf8(attr.value.to_vec())
                            && let Some(item) = cur.as_mut()
                        {
                            item.components.push(value);
                        }
                    }
                    "getetag" | "href" | "displayname" => {
                        // Self-closing variants carry no text; nothing to do.
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if capture.is_some() {
                    let decoded = t
                        .xml_content()
                        .map_err(|e| format!("Bad XML text from CalDAV server: {e}"))?;
                    text.push_str(&decoded);
                }
            }
            // quick-xml 0.38 splits entity references out of text into their
            // own events (e.g. &quot; in ETags, &#xD;&#xA; in iCalendar data).
            Event::GeneralRef(r) => {
                if capture.is_some() {
                    let resolved = match r.resolve_char_ref() {
                        Ok(Some(ch)) => Some(ch.to_string()),
                        Ok(None) => {
                            let name = std::str::from_utf8(r.as_ref()).unwrap_or("");
                            match name {
                                "amp" => Some("&".to_string()),
                                "lt" => Some("<".to_string()),
                                "gt" => Some(">".to_string()),
                                "quot" => Some("\"".to_string()),
                                "apos" => Some("'".to_string()),
                                _ => None,
                            }
                        }
                        Err(_) => None,
                    };
                    if let Some(fragment) = resolved {
                        text.push_str(&fragment);
                    }
                }
            }
            Event::End(e) => {
                let name = xml_local_name(e.local_name().as_ref()).to_string();
                if let Some(field) = capture.take()
                    && field == name
                    && let Some(item) = cur.as_mut()
                {
                    match field.as_str() {
                        "href" => item.href = text.clone(),
                        "displayname" => item.displayname = Some(text.clone()),
                        "getetag" => item.etag = Some(text.clone()),
                        "calendar-data" => item.calendar_data = Some(text.clone()),
                        _ => {}
                    }
                }
                text.clear();
                path.pop();
                if name == "response"
                    && let Some(item) = cur.take()
                {
                    items.push(item);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_ical_datetime() {
        assert_eq!(compact_ical_datetime("2026-03-01T00:00:00Z"), "20260301T000000Z");
        assert_eq!(compact_ical_datetime("2026-03-01"), "20260301T000000Z");
    }

    #[test]
    fn test_parse_multistatus_collections() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav.php/calendars/kayden/default/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Default</D:displayname>
        <D:resourcetype><D:collection/><cal:calendar/></D:resourcetype>
        <cal:supported-calendar-component-set>
          <cal:comp name="VEVENT"/>
          <cal:comp name="VTODO"/>
        </cal:supported-calendar-component-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav.php/calendars/kayden/inbox/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Schedule Inbox</D:displayname>
        <D:resourcetype><D:collection/></D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let items = parse_multistatus(xml).unwrap();
        assert_eq!(items.len(), 2);

        assert!(items[0].is_calendar);
        assert_eq!(items[0].displayname.as_deref(), Some("Default"));
        assert_eq!(items[0].components, vec!["VEVENT", "VTODO"]);

        assert!(!items[1].is_calendar);
    }

    #[test]
    fn test_parse_multistatus_resources_with_calendar_data() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/dav.php/calendars/kayden/default/abc.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>&quot;e123&quot;</D:getetag>
        <cal:calendar-data>BEGIN:VCALENDAR&#13;&#10;VERSION:2.0&#13;&#10;END:VCALENDAR</cal:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let items = parse_multistatus(xml).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].etag.as_deref(), Some("\"e123\""));
        let ics = items[0].calendar_data.as_deref().unwrap();
        assert!(ics.contains("BEGIN:VCALENDAR"));
        assert!(ics.contains("END:VCALENDAR"));
    }

    #[test]
    fn test_resolve_href_relative_and_absolute() {
        let base = Url::parse("https://cal.example.com/dav.php/calendars/kayden/default/").unwrap();
        let rel = resolve_href(&base, "/dav.php/calendars/kayden/default/abc.ics").unwrap();
        assert_eq!(rel, "https://cal.example.com/dav.php/calendars/kayden/default/abc.ics");
        let abs = resolve_href(&base, "https://other.example/x.ics").unwrap();
        assert_eq!(abs, "https://other.example/x.ics");
    }
}
