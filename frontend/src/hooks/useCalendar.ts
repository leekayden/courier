"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { apiGet, apiPost, apiPut, apiDelete } from "@/lib/api";
import type {
  CalendarEvent,
  CalendarEventsResponse,
  CalendarStatus,
  CalendarSettings,
  CalendarAccount,
  CalendarAccountSummary,
  CreateCalendarAccountRequest,
  UpdateCalendarAccountRequest,
  CalDavCollection,
  MeetingTemplate,
  MeetingTemplatesResponse,
} from "@/types/calendar";

/**
 * Whether the calendar is configured. The calendar stays disabled until the
 * user sets up a provider account (or has legacy local events).
 */
export function useCalendarStatus() {
  return useQuery({
    queryKey: ["calendar-status"],
    queryFn: () => apiGet<CalendarStatus>("/calendar/status"),
  });
}

/**
 * Events + todos for a time range. The backend syncs configured provider
 * accounts (CalDAV / Google) into the cache before responding; serving is
 * read-only.
 */
export function useCalendarEvents(start: string, end: string) {
  return useQuery({
    queryKey: ["calendar-events", start, end],
    queryFn: () =>
      apiGet<CalendarEventsResponse>(
        `/calendar/events?start=${encodeURIComponent(start)}&end=${encodeURIComponent(end)}`,
      ),
    enabled: !!start && !!end,
  });
}

export function useCalendarEvent(id: string | null) {
  return useQuery({
    queryKey: ["calendar-event", id],
    queryFn: () => apiGet<CalendarEvent>(`/calendar/events/${id}`),
    enabled: !!id,
  });
}

// ---------------------------------------------------------------------------
// Provider accounts
// ---------------------------------------------------------------------------

export function useCalendarAccounts() {
  return useQuery({
    queryKey: ["calendar-accounts"],
    queryFn: () =>
      apiGet<{ accounts: CalendarAccount[] }>("/calendar/accounts"),
  });
}

function invalidateCalendarData(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({ queryKey: ["calendar-accounts"] });
  queryClient.invalidateQueries({ queryKey: ["calendar-status"] });
  queryClient.invalidateQueries({ queryKey: ["calendar-events"] });
}

export function useCreateCalendarAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateCalendarAccountRequest) =>
      apiPost<CalendarAccount>("/calendar/accounts", body),
    onSuccess: () => {
      invalidateCalendarData(queryClient);
    },
  });
}

export function useUpdateCalendarAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...body }: UpdateCalendarAccountRequest & { id: number }) =>
      apiPut<CalendarAccount>(`/calendar/accounts/${id}`, body),
    onSuccess: () => {
      invalidateCalendarData(queryClient);
    },
  });
}

export function useDeleteCalendarAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => apiDelete(`/calendar/accounts/${id}`),
    onSuccess: () => {
      invalidateCalendarData(queryClient);
    },
  });
}

/** Probe a CalDAV URL and list the calendar collections it exposes. */
export function useCalDavDiscover() {
  return useMutation({
    mutationFn: (body: { url: string; username: string; secret: string }) =>
      apiPost<{ collections: CalDavCollection[] }>(
        "/calendar/caldav/discover",
        body,
      ),
  });
}

/** Whether the server has Google OAuth credentials configured. */
export function useGoogleOAuthStatus() {
  return useQuery({
    queryKey: ["calendar-google-status"],
    queryFn: () =>
      apiGet<{ google_oauth_enabled: boolean }>("/calendar/google/status"),
  });
}

/** Fetch the Google consent URL. Returns it so the caller can redirect. */
export async function fetchGoogleAuthUrl(): Promise<{ url: string }> {
  return apiGet<{ url: string }>("/calendar/google/auth-url");
}

// ---------------------------------------------------------------------------
// Settings (display preferences) + meeting templates
// ---------------------------------------------------------------------------

export function useCalendarSettings() {
  return useQuery({
    queryKey: ["calendar-settings"],
    queryFn: () => apiGet<CalendarSettings>("/calendar/settings"),
  });
}

export function useUpdateCalendarSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: Partial<CalendarSettings>) =>
      apiPut<CalendarSettings>(
        "/calendar/settings",
        body as Record<string, unknown>,
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["calendar-settings"] });
    },
  });
}

export function useMeetingTemplates() {
  return useQuery({
    queryKey: ["meeting-templates"],
    queryFn: () =>
      apiGet<MeetingTemplatesResponse>("/calendar/meeting-templates"),
  });
}

export function useCreateMeetingTemplate() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: {
      name: string;
      url_template: string;
      icon?: string;
    }) =>
      apiPost<MeetingTemplate>(
        "/calendar/meeting-templates",
        body as Record<string, unknown>,
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["meeting-templates"] });
    },
  });
}

export function useDeleteMeetingTemplate() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: number) =>
      apiDelete(`/calendar/meeting-templates/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["meeting-templates"] });
    },
  });
}

export type { CalendarAccountSummary };
