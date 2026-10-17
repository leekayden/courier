use ical::parser::ical::component::IcalCalendar;
use ical::IcalParser;
use std::io::BufReader;

use crate::db::calendar::{RemoteEvent, RemoteTodo};

/// Parse iCalendar text into remote events and todos ready for the cache.
///
/// Events without a DTSTART are skipped. Todos without a DTSTART or DUE are
/// still kept (undated todos are legal in iCalendar and shown in the Tasks
/// list). Datetimes are kept in their wire format: `Z`-suffixed UTC values
/// stay UTC, TZID-qualified values keep their naive local representation.
pub fn parse_ics_full(ics_text: &str) -> Result<(Vec<RemoteEvent>, Vec<RemoteTodo>), String> {
    let reader = BufReader::new(ics_text.as_bytes());
    let parser = IcalParser::new(reader);

    let mut events = Vec::new();
    let mut todos = Vec::new();

    for calendar_result in parser {
        let calendar: IcalCalendar =
            calendar_result.map_err(|e| format!("Failed to parse ICS: {e}"))?;

        for vevent in calendar.events {
            if let Some(event) = parse_vevent(&vevent.properties) {
                events.push(event);
            }
        }

        for vtodo in calendar.todos {
            if let Some(todo) = parse_vtodo(&vtodo.properties) {
                todos.push(todo);
            }
        }
    }

    Ok((events, todos))
}

type Props = Vec<ical::property::Property>;

fn parse_vevent(props: &Props) -> Option<RemoteEvent> {
    let mut title = String::new();
    let mut description = String::new();
    let mut location = String::new();
    let mut start_time = String::new();
    let mut end_time = String::new();
    let mut all_day = false;
    let mut uid = String::new();
    let mut rrule = None;
    let mut organizer = String::new();
    let mut attendees: Vec<String> = Vec::new();
    let mut url = None;
    let mut status = "confirmed".to_string();

    for prop in props {
        let value = prop.value.as_deref().unwrap_or("");

        match prop.name.as_str() {
            "SUMMARY" => title = value.to_string(),
            "DESCRIPTION" => description = value.to_string(),
            "LOCATION" => location = value.to_string(),
            "DTSTART" => {
                all_day = is_date_only(value);
                start_time = parse_ical_datetime(value, &prop.params);
            }
            "DTEND" => end_time = parse_ical_datetime(value, &prop.params),
            "UID" => uid = value.to_string(),
            "RRULE" => rrule = Some(value.to_string()),
            "ORGANIZER" => organizer = extract_email_from_cal_address(value),
            "ATTENDEE" => attendees.push(extract_email_from_cal_address(value)),
            "URL" if !value.is_empty() => url = Some(value.to_string()),
            "STATUS" if !value.is_empty() => status = value.to_lowercase(),
            _ => {}
        }
    }

    if start_time.is_empty() {
        return None;
    }
    if title.is_empty() {
        title = "(No title)".to_string();
    }
    if end_time.is_empty() {
        end_time = start_time.clone();
    }

    let attendees_json =
        serde_json::to_string(&attendees).unwrap_or_else(|_| "[]".to_string());

    Some(RemoteEvent {
        uid,
        title,
        description,
        location,
        start_time,
        end_time,
        all_day,
        recurrence_rule: rrule.filter(|r| !r.is_empty()),
        attendees: attendees_json,
        organizer,
        status,
        meeting_url: url,
        // Filled in by the provider with the resource href.
        remote_href: String::new(),
        remote_etag: None,
    })
}

fn parse_vtodo(props: &Props) -> Option<RemoteTodo> {
    let mut title = String::new();
    let mut description = String::new();
    let mut start_time = String::new();
    let mut due_time = String::new();
    let mut uid = String::new();
    let mut url = None;
    let mut status = "needs-action".to_string();
    let mut completed = false;
    let mut completed_at = None;
    let mut percent_complete = None;
    let mut priority = None;

    for prop in props {
        let value = prop.value.as_deref().unwrap_or("");

        match prop.name.as_str() {
            "SUMMARY" => title = value.to_string(),
            "DESCRIPTION" => description = value.to_string(),
            "DTSTART" => start_time = parse_ical_datetime(value, &prop.params),
            "DUE" => due_time = parse_ical_datetime(value, &prop.params),
            "UID" => uid = value.to_string(),
            "URL" if !value.is_empty() => url = Some(value.to_string()),
            "STATUS" if !value.is_empty() => {
                status = value.to_lowercase();
                completed = status == "completed";
            }
            "COMPLETED" if !value.is_empty() => {
                completed_at = Some(parse_ical_datetime(value, &prop.params));
                completed = true;
            }
            "PERCENT-COMPLETE" => percent_complete = value.parse().ok(),
            "PRIORITY" => priority = value.parse().ok().filter(|p| *p > 0),
            _ => {}
        }
    }

    if title.is_empty() {
        title = "(No title)".to_string();
    }

    Some(RemoteTodo {
        uid,
        title,
        description,
        start_time,
        due_time,
        status,
        completed,
        completed_at,
        percent_complete,
        priority,
        url,
        // Filled in by the provider with the resource href.
        remote_href: String::new(),
        remote_etag: None,
    })
}

/// Check if a DTSTART/DTEND value is date-only (no time component).
fn is_date_only(value: &str) -> bool {
    // Date-only values are 8 characters: YYYYMMDD
    // Date-time values are at least 15: YYYYMMDDTHHmmSS
    value.len() == 8 && value.chars().all(|c| c.is_ascii_digit())
}

/// Parse an iCalendar date/datetime value to ISO 8601 format.
fn parse_ical_datetime(
    value: &str,
    params: &Option<Vec<(String, Vec<String>)>>,
) -> String {
    // Check for TZID parameter
    let _tzid = params.as_ref().and_then(|ps| {
        ps.iter()
            .find(|(k, _)| k == "TZID")
            .and_then(|(_, v)| v.first())
            .cloned()
    });

    let value = value.trim();

    if value.len() == 8 && value.chars().all(|c| c.is_ascii_digit()) {
        // Date only: YYYYMMDD -> YYYY-MM-DD
        return format!(
            "{}-{}-{}",
            &value[0..4],
            &value[4..6],
            &value[6..8]
        );
    }

    if value.len() >= 15 && value.contains('T') {
        // DateTime: YYYYMMDDTHHmmSS or YYYYMMDDTHHmmSSZ
        let date_part = &value[0..8];
        let time_part = &value[9..];

        let year = &date_part[0..4];
        let month = &date_part[4..6];
        let day = &date_part[6..8];

        let hour = &time_part[0..2];
        let min = &time_part[2..4];
        let sec = if time_part.len() >= 6 {
            &time_part[4..6]
        } else {
            "00"
        };

        let is_utc = value.ends_with('Z');
        let suffix = if is_utc { "Z" } else { "" };

        return format!("{year}-{month}-{day}T{hour}:{min}:{sec}{suffix}");
    }

    // Fallback: return as-is
    value.to_string()
}

/// Extract email address from a cal-address value (e.g., "mailto:user@example.com").
fn extract_email_from_cal_address(value: &str) -> String {
    if let Some(stripped) = value.strip_prefix("mailto:") {
        stripped.to_string()
    } else if let Some(stripped) = value.strip_prefix("MAILTO:") {
        stripped.to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_ICS: &str = r#"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Test//Test//EN
BEGIN:VEVENT
DTSTART:20250306T090000Z
DTEND:20250306T100000Z
SUMMARY:Team Meeting
DESCRIPTION:Weekly team sync
LOCATION:Conference Room A
UID:event-123@example.com
ORGANIZER:mailto:boss@example.com
ATTENDEE:mailto:alice@example.com
ATTENDEE:mailto:bob@example.com
URL:https://zoom.us/j/12345
STATUS:CONFIRMED
END:VEVENT
BEGIN:VTODO
UID:todo-1@example.com
SUMMARY:Send invoice
DUE:20250310T170000Z
PRIORITY:5
PERCENT-COMPLETE:40
STATUS:NEEDS-ACTION
END:VTODO
BEGIN:VTODO
UID:todo-2@example.com
SUMMARY:Undated idea
STATUS:COMPLETED
COMPLETED:20250308T120000Z
END:VTODO
END:VCALENDAR"#;

    #[test]
    fn test_parse_full_with_events_and_todos() {
        let (events, todos) = parse_ics_full(SAMPLE_ICS).unwrap();
        assert_eq!(events.len(), 1);

        let event = &events[0];
        assert_eq!(event.title, "Team Meeting");
        assert_eq!(event.description, "Weekly team sync");
        assert_eq!(event.location, "Conference Room A");
        assert_eq!(event.start_time, "2025-03-06T09:00:00Z");
        assert_eq!(event.end_time, "2025-03-06T10:00:00Z");
        assert!(!event.all_day);
        assert_eq!(event.organizer, "boss@example.com");
        assert_eq!(event.uid, "event-123@example.com");
        assert_eq!(event.meeting_url.as_deref(), Some("https://zoom.us/j/12345"));
        assert_eq!(event.status, "confirmed");

        let attendees: Vec<String> = serde_json::from_str(&event.attendees).unwrap();
        assert_eq!(attendees.len(), 2);
        assert!(attendees.contains(&"alice@example.com".to_string()));
        assert!(attendees.contains(&"bob@example.com".to_string()));

        assert_eq!(todos.len(), 2);

        let invoice = &todos[0];
        assert_eq!(invoice.title, "Send invoice");
        assert_eq!(invoice.due_time, "2025-03-10T17:00:00Z");
        assert_eq!(invoice.status, "needs-action");
        assert!(!invoice.completed);
        assert_eq!(invoice.percent_complete, Some(40));
        assert_eq!(invoice.priority, Some(5));

        let idea = &todos[1];
        assert!(idea.due_time.is_empty(), "undated todos are kept");
        assert!(idea.completed);
        assert_eq!(idea.completed_at.as_deref(), Some("2025-03-08T12:00:00Z"));
    }

    #[test]
    fn test_parse_all_day_event() {
        let ics = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
DTSTART;VALUE=DATE:20250306
DTEND;VALUE=DATE:20250307
SUMMARY:All Day Event
UID:allday-1@example.com
END:VEVENT
END:VCALENDAR"#;

        let (events, todos) = parse_ics_full(ics).unwrap();
        assert!(todos.is_empty());
        assert_eq!(events.len(), 1);
        assert!(events[0].all_day);
        assert_eq!(events[0].start_time, "2025-03-06");
        assert_eq!(events[0].end_time, "2025-03-07");
    }

    #[test]
    fn test_parse_multiple_events_and_rrule() {
        let ics = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
DTSTART:20250306T090000Z
DTEND:20250306T100000Z
SUMMARY:Event 1
UID:e1@example.com
RRULE:FREQ=WEEKLY;COUNT=3
END:VEVENT
BEGIN:VEVENT
DTSTART:20250306T140000Z
DTEND:20250306T150000Z
SUMMARY:Event 2
UID:e2@example.com
END:VEVENT
END:VCALENDAR"#;

        let (events, _) = parse_ics_full(ics).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].recurrence_rule.as_deref(), Some("FREQ=WEEKLY;COUNT=3"));
    }

    #[test]
    fn test_event_without_dtstart_skipped() {
        let ics = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
SUMMARY:No start
UID:no-start@example.com
END:VEVENT
END:VCALENDAR"#;
        let (events, _) = parse_ics_full(ics).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_ical_datetime_utc() {
        assert_eq!(
            parse_ical_datetime("20250306T090000Z", &None),
            "2025-03-06T09:00:00Z"
        );
    }

    #[test]
    fn test_parse_ical_datetime_local() {
        assert_eq!(
            parse_ical_datetime("20250306T090000", &None),
            "2025-03-06T09:00:00"
        );
    }

    #[test]
    fn test_parse_ical_date_only() {
        assert_eq!(parse_ical_datetime("20250306", &None), "2025-03-06");
    }

    #[test]
    fn test_extract_email() {
        assert_eq!(
            extract_email_from_cal_address("mailto:user@example.com"),
            "user@example.com"
        );
        assert_eq!(
            extract_email_from_cal_address("MAILTO:user@example.com"),
            "user@example.com"
        );
        assert_eq!(
            extract_email_from_cal_address("user@example.com"),
            "user@example.com"
        );
    }
}
