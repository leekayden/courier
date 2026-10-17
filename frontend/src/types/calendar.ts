/** A calendar event (or todo) returned by the API. */
export type CalendarEvent = {
  id: string;
  title: string;
  description: string;
  location: string;
  start_time: string;
  end_time: string;
  all_day: boolean;
  recurrence_rule: string | null;
  attendees: string;
  organizer: string;
  status: string;
  source: string;
  source_uid: string | null;
  meeting_url: string | null;
  color: string | null;
  reminder_minutes: number | null;
  account_id: number | null;
  account_name: string | null;
  item_kind: "event" | "todo";
  percent_complete: number | null;
  priority: number | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
};

/** A configured calendar provider account (secrets are never returned). */
export type CalendarAccount = {
  id: number;
  provider: "caldav" | "google";
  name: string;
  color: string;
  url: string;
  username: string;
  enabled: boolean;
  has_secret: boolean;
  last_synced_at: string | null;
  last_sync_error: string | null;
  created_at: string;
  updated_at: string;
};

/** Body for POST /calendar/accounts. */
export type CreateCalendarAccountRequest = {
  provider: "caldav" | "google";
  name?: string;
  color?: string;
  url?: string;
  username?: string;
  secret?: string;
};

/** Body for PUT /calendar/accounts/:id. */
export type UpdateCalendarAccountRequest = {
  name?: string;
  color?: string;
  url?: string;
  username?: string;
  secret?: string;
  enabled?: boolean;
};

/** A calendar collection found via CalDAV discovery. */
export type CalDavCollection = {
  href: string;
  displayname: string;
  components: string[];
};

/** GET /calendar/status response. */
export type CalendarStatus = {
  configured: boolean;
  account_count: number;
  enabled_account_count: number;
  legacy_event_count: number;
  /** Whether the server has Google OAuth credentials (GOOGLE_CLIENT_ID/SECRET). */
  google_oauth_enabled: boolean;
};

/** User calendar preferences. */
export type CalendarSettings = {
  default_view: string;
  week_starts_on: number;
  working_hours_start: string;
  working_hours_end: string;
  time_format: string;
  zoom_link: string;
  teams_link: string;
  meet_link: string;
};

/** A meeting link template. */
export type MeetingTemplate = {
  id: number;
  name: string;
  url_template: string;
  icon: string;
  is_default: boolean;
};

/** Minimal account info attached to GET /events for UI labels. */
export type CalendarAccountSummary = {
  id: number;
  provider: string;
  name: string;
  color: string;
  enabled: boolean;
  last_synced_at: string | null;
  last_sync_error: string | null;
};

/** Response from GET /events (read-only; synced from provider accounts). */
export type CalendarEventsResponse = {
  configured: boolean;
  events: CalendarEvent[];
  todos: CalendarEvent[];
  accounts: CalendarAccountSummary[];
  sync_errors: Record<string, string>;
};

/** Response from GET /meeting-templates. */
export type MeetingTemplatesResponse = {
  templates: MeetingTemplate[];
};
