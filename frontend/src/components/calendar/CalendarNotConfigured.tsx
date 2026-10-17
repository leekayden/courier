"use client";

import { CalendarDays, Lock, Settings2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useUiStore } from "@/stores/useUiStore";

type CalendarNotConfiguredProps = {
  /** Whether the server has Google OAuth credentials for the Google option. */
  googleOAuthEnabled?: boolean;
};

/**
 * Empty state shown while the calendar is disabled (no provider accounts
 * configured). The calendar is read-only and only displays events and tasks
 * from external providers — nothing is stored locally until one is set up.
 */
export function CalendarNotConfigured({
  googleOAuthEnabled,
}: CalendarNotConfiguredProps) {
  const setViewMode = useUiStore((s) => s.setViewMode);
  const setPendingSettingsTab = useUiStore((s) => s.setPendingSettingsTab);

  const openCalendarSettings = () => {
    setPendingSettingsTab("calendar");
    setViewMode("settings");
  };

  return (
    <div
      className="flex flex-1 items-center justify-center overflow-y-auto p-6"
      data-testid="calendar-not-configured"
    >
      <div className="w-full max-w-md text-center">
        <div className="mx-auto mb-5 flex size-14 items-center justify-center rounded-2xl bg-muted">
          <CalendarDays className="size-7 text-muted-foreground" />
        </div>
        <h2 className="text-lg font-semibold text-foreground">
          Calendar is disabled
        </h2>
        <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
          Connect a calendar provider to see your events and tasks here.
          Courier only reads from your calendar — it never creates, edits, or
          deletes anything on it.
        </p>

        <div className="mt-5 space-y-2 text-left">
          <div className="rounded-lg border border-border p-3">
            <div className="text-sm font-medium text-foreground">CalDAV</div>
            <div className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
              Works with self-hosted servers like Baikal, Nextcloud, and
              Radicale — e.g.
              <code className="mt-1 block truncate rounded bg-muted px-1.5 py-0.5 font-mono text-[11px]">
                https://cal.example.com/dav.php/calendars/user/default/
              </code>
            </div>
          </div>
          <div className="rounded-lg border border-border p-3">
            <div className="flex items-center justify-between gap-2">
              <div className="text-sm font-medium text-foreground">
                Google Calendar
              </div>
              {!googleOAuthEnabled && (
                <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
                  Server setup required
                </span>
              )}
            </div>
            <div className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
              {googleOAuthEnabled
                ? "Connect your Google account (read-only access to calendars and tasks)."
                : "An administrator must set GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET on the server to enable this option."}
            </div>
          </div>
        </div>

        <Button onClick={openCalendarSettings} className="mt-6 gap-1.5">
          <Settings2 className="size-4" />
          Set up a calendar
        </Button>

        <p className="mt-4 flex items-center justify-center gap-1.5 text-xs text-muted-foreground">
          <Lock className="size-3" />
          Read-only by design — your data stays on your server.
        </p>
      </div>
    </div>
  );
}
