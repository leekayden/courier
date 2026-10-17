"use client";

import { useEffect, useState } from "react";
import { toast } from "sonner";
import {
  CalendarDays,
  Check,
  Loader2,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Chip } from "@/components/ui/Chip";
import {
  useCalendarAccounts,
  useCalendarStatus,
  useCreateCalendarAccount,
  useUpdateCalendarAccount,
  useDeleteCalendarAccount,
  useCalDavDiscover,
  fetchGoogleAuthUrl,
} from "@/hooks/useCalendar";
import { EVENT_COLORS, getEventColorClasses } from "@/components/calendar/calendarUtils";
import { cn } from "@/lib/utils";
import type { CalendarAccount, CalDavCollection } from "@/types/calendar";

const EXAMPLE_CALDAV_URL =
  "https://cal.cloudservetechcentral.com/dav.php/calendars/kayden/default/";

/** OAuth redirect landing feedback: the Google callback returns to /mail with
 * a `calendar=connected` or `calendar_error=...` query param. */
function useOAuthRedirectFeedback() {
  useEffect(() => {
    if (typeof window === "undefined") return;
    const params = new URLSearchParams(window.location.search);
    if (params.get("calendar") === "connected") {
      toast.success("Google Calendar connected");
      params.delete("calendar");
    }
    const error = params.get("calendar_error");
    if (error) {
      const messages: Record<string, string> = {
        missing_code_or_state: "Google sign-in was not completed.",
        invalid_or_expired_state: "The Google sign-in attempt expired. Please try again.",
        token_exchange_failed: "Could not authorize with Google. Please try again.",
        storage_failed: "Connected, but saving the account failed. Please try again.",
        google_not_configured: "Google Calendar is not configured on this server.",
      };
      toast.error(messages[error] ?? "Google sign-in failed.");
      params.delete("calendar_error");
    }
    if (params.get("calendar") || params.get("calendar_error")) {
      const rest = params.toString();
      window.history.replaceState(
        null,
        "",
        `${window.location.pathname}${rest ? `?${rest}` : ""}`,
      );
    }
  }, []);
}

type ProviderChoice = "caldav" | "google";

export function CalendarAccountsSettings() {
  useOAuthRedirectFeedback();

  const { data: status } = useCalendarStatus();
  const { data: accountsData, isLoading } = useCalendarAccounts();
  const accounts = accountsData?.accounts ?? [];
  const createAccount = useCreateCalendarAccount();
  const updateAccount = useUpdateCalendarAccount();
  const deleteAccount = useDeleteCalendarAccount();
  const discover = useCalDavDiscover();

  const [adding, setAdding] = useState<ProviderChoice | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [deleteConfirmId, setDeleteConfirmId] = useState<number | null>(null);

  // CalDAV form state (shared by add + edit).
  const [caldavUrl, setCaldavUrl] = useState("");
  const [caldavUsername, setCaldavUsername] = useState("");
  const [caldavPassword, setCaldavPassword] = useState("");
  const [caldavName, setCaldavName] = useState("");
  const [caldavColor, setCaldavColor] = useState("blue");
  const [collections, setCollections] = useState<CalDavCollection[]>([]);
  const [selectedHref, setSelectedHref] = useState("");
  const [formError, setFormError] = useState<string | null>(null);

  const editingAccount = accounts.find((a) => a.id === editingId) ?? null;

  const resetForm = () => {
    setAdding(null);
    setEditingId(null);
    setCaldavUrl("");
    setCaldavUsername("");
    setCaldavPassword("");
    setCaldavName("");
    setCaldavColor("blue");
    setCollections([]);
    setSelectedHref("");
    setFormError(null);
  };

  const startEdit = (account: CalendarAccount) => {
    setAdding(account.provider === "caldav" ? "caldav" : "google");
    setEditingId(account.id);
    setCaldavUrl(account.url === "primary" ? "" : account.url);
    setCaldavUsername(account.username);
    setCaldavPassword("");
    setCaldavName(account.name);
    setCaldavColor(account.color || "blue");
    setCollections([]);
    setSelectedHref("");
    setFormError(null);
  };

  const handleDiscover = async () => {
    if (!caldavUrl.trim() || !caldavUsername.trim() || !caldavPassword.trim()) {
      setFormError("URL, username, and password are needed to search for calendars.");
      return;
    }
    setFormError(null);
    try {
      const result = await discover.mutateAsync({
        url: caldavUrl.trim(),
        username: caldavUsername.trim(),
        secret: caldavPassword,
      });
      setCollections(result.collections);
      if (result.collections.length === 0) {
        setFormError("No calendars were found at that URL. Check the address and credentials.");
      } else {
        setSelectedHref(result.collections[0].href);
        const first = result.collections[0];
        if (!caldavName.trim() && first.displayname) setCaldavName(first.displayname);
      }
    } catch (e) {
      setFormError(e instanceof Error ? e.message : "Could not reach the CalDAV server.");
    }
  };

  const handleSaveCaldav = async () => {
    const url = (selectedHref || caldavUrl).trim();
    if (!url) {
      setFormError("A calendar URL is required.");
      return;
    }
    if (!caldavUsername.trim() || (!editingId && !caldavPassword.trim())) {
      setFormError("Username and password are required.");
      return;
    }

    const payload = {
      name: caldavName.trim(),
      color: caldavColor,
      url,
      username: caldavUsername.trim(),
      secret: caldavPassword.trim(),
    };

    try {
      if (editingId !== null) {
        await updateAccount.mutateAsync({ id: editingId, ...payload });
        toast.success("Calendar account updated");
      } else {
        await createAccount.mutateAsync({ provider: "caldav", ...payload });
        toast.success("Calendar connected — syncing your events");
      }
      resetForm();
    } catch (e) {
      setFormError(e instanceof Error ? e.message : "Failed to save the calendar account.");
    }
  };

  const handleConnectGoogle = async () => {
    setFormError(null);
    try {
      const { url } = await fetchGoogleAuthUrl();
      window.location.href = url;
    } catch (e) {
      setFormError(
        e instanceof Error ? e.message : "Could not start the Google sign-in flow.",
      );
    }
  };

  const handleToggleEnabled = (account: CalendarAccount, enabled: boolean) => {
    updateAccount.mutate(
      { id: account.id, enabled },
      {
        onError: (e) => toast.error(`Failed to update: ${e.message}`),
      },
    );
  };

  const handleDelete = (account: CalendarAccount) => {
    deleteAccount.mutate(account.id, {
      onSuccess: () => toast.success(`Removed "${account.name || account.username}"`),
      onError: (e) => toast.error(`Failed to remove: ${e.message}`),
      onSettled: () => setDeleteConfirmId(null),
    });
  };

  const googleEnabled = status?.google_oauth_enabled ?? false;
  const saving = createAccount.isPending || updateAccount.isPending;

  return (
    <div className="mx-auto max-w-2xl space-y-4" data-testid="calendar-settings">
      <div>
        <h2 className="text-base font-semibold text-foreground">Calendar accounts</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Connect a calendar provider to see its events and tasks in the
          calendar. Access is read-only — Courier never modifies your calendar.
        </p>
      </div>

      {/* Account list */}
      {isLoading ? (
        <div className="flex justify-center py-8">
          <Loader2 className="size-5 animate-spin text-muted-foreground" />
        </div>
      ) : accounts.length === 0 && adding === null ? (
        <div className="rounded-lg border border-dashed border-border p-6 text-center">
          <CalendarDays className="mx-auto mb-2 size-6 text-muted-foreground" />
          <p className="text-sm font-medium text-foreground">No calendar connected</p>
          <p className="mt-1 text-xs text-muted-foreground">
            The calendar stays disabled until you connect a provider.
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {accounts.map((account) => (
            <div
              key={account.id}
              className="rounded-lg border border-border p-4"
              data-testid={`calendar-account-${account.id}`}
            >
              <div className="flex items-center gap-3">
                <span
                  className={cn("size-3 shrink-0 rounded-full", getEventColorClasses(account.color).bg)}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium text-foreground">
                      {account.name || account.username || "Calendar"}
                    </span>
                    <Chip variant="muted">{account.provider === "caldav" ? "CalDAV" : "Google"}</Chip>
                    {!account.enabled && <Chip variant="muted">Off</Chip>}
                  </div>
                  <div className="truncate text-xs text-muted-foreground">
                    {account.username}
                    {account.provider === "caldav" && account.url ? ` — ${account.url}` : ""}
                  </div>
                </div>
                <Switch
                  checked={account.enabled}
                  onChange={(checked) => handleToggleEnabled(account, checked)}
                  aria-label={`Toggle ${account.name || "calendar account"}`}
                />
                {account.provider === "caldav" && (
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label="Edit account"
                    onClick={() => startEdit(account)}
                  >
                    <Pencil className="size-3.5" />
                  </Button>
                )}
                <div className="relative">
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label="Remove account"
                    className="text-muted-foreground hover:text-destructive"
                    onClick={() => setDeleteConfirmId((v) => (v === account.id ? null : account.id))}
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                  {deleteConfirmId === account.id && (
                    <div className="absolute right-0 top-8 z-10 w-56 rounded-lg border border-border bg-background p-3 shadow-lg">
                      <p className="mb-2 text-xs font-medium text-foreground">
                        Remove this account and its cached events?
                      </p>
                      <div className="flex justify-end gap-2">
                        <Button variant="outline" size="sm" onClick={() => setDeleteConfirmId(null)}>
                          Cancel
                        </Button>
                        <Button
                          variant="destructive"
                          size="sm"
                          disabled={deleteAccount.isPending}
                          onClick={() => handleDelete(account)}
                        >
                          Remove
                        </Button>
                      </div>
                    </div>
                  )}
                </div>
              </div>
              {account.last_sync_error && (
                <p className="mt-2 break-words text-xs text-destructive">
                  {account.last_sync_error}
                </p>
              )}
              {account.last_synced_at && !account.last_sync_error && (
                <p className="mt-1.5 flex items-center gap-1 text-[10px] text-muted-foreground">
                  <RefreshCw className="size-2.5" />
                  Last synced{" "}
                  {new Date(account.last_synced_at).toLocaleString("en-US", {
                    month: "short",
                    day: "numeric",
                    hour: "numeric",
                    minute: "2-digit",
                  })}
                </p>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Add buttons */}
      {adding === null ? (
        <div className="flex gap-2">
          <Button variant="outline" onClick={() => { setAdding("caldav"); setFormError(null); }} className="gap-1.5">
            <Plus className="size-4" />
            Add CalDAV
          </Button>
          <Button
            variant="outline"
            onClick={() => (googleEnabled ? handleConnectGoogle() : setAdding("google"))}
            disabled={!googleEnabled}
            title={googleEnabled ? undefined : "Requires GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET on the server"}
            className="gap-1.5"
          >
            <Plus className="size-4" />
            Connect Google Calendar
          </Button>
        </div>
      ) : adding === "caldav" ? (
        /* ---------------- CalDAV form ---------------- */
        <div className="space-y-3 rounded-lg border border-border p-4">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-foreground">
              {editingId !== null ? "Edit CalDAV account" : "Add CalDAV account"}
            </h3>
            <Button variant="ghost" size="icon-sm" aria-label="Cancel" onClick={resetForm}>
              <X className="size-4" />
            </Button>
          </div>

          <div className="space-y-1">
            <Label htmlFor="caldav-url">Server URL</Label>
            <Input
              id="caldav-url"
              type="url"
              placeholder={EXAMPLE_CALDAV_URL}
              value={caldavUrl}
              onChange={(e) => setCaldavUrl(e.target.value)}
              disabled={editingId !== null && !!editingAccount}
            />
            <p className="text-[11px] text-muted-foreground">
              Your calendar&apos;s CalDAV URL (Baikal, Nextcloud, Radicale, …).
            </p>
          </div>

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="caldav-username">Username</Label>
              <Input
                id="caldav-username"
                type="text"
                autoComplete="off"
                value={caldavUsername}
                onChange={(e) => setCaldavUsername(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="caldav-password">
                Password {editingId !== null && <span className="font-normal text-muted-foreground">(leave blank to keep)</span>}
              </Label>
              <Input
                id="caldav-password"
                type="password"
                autoComplete="new-password"
                value={caldavPassword}
                onChange={(e) => setCaldavPassword(e.target.value)}
              />
            </div>
          </div>

          {editingId === null && (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={handleDiscover}
              disabled={discover.isPending}
              className="gap-1.5"
            >
              {discover.isPending ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Search className="size-3.5" />
              )}
              Find calendars
            </Button>
          )}

          {collections.length > 0 && (
            <div className="space-y-1">
              <Label>Calendar</Label>
              <div className="space-y-1">
                {collections.map((collection) => {
                  const supportsEvents = collection.components.length === 0
                    || collection.components.includes("VEVENT")
                    || collection.components.includes("VTODO");
                  return (
                    <button
                      key={collection.href}
                      type="button"
                      onClick={() => setSelectedHref(collection.href)}
                      className={cn(
                        "flex w-full items-center gap-2 rounded-md border px-3 py-2 text-left text-sm transition-colors",
                        selectedHref === collection.href
                          ? "border-primary bg-primary/5"
                          : "border-border hover:bg-accent",
                      )}
                    >
                      <span
                        className={cn(
                          "flex size-4 shrink-0 items-center justify-center rounded-full border",
                          selectedHref === collection.href ? "border-primary bg-primary" : "border-muted-foreground/40",
                        )}
                      >
                        {selectedHref === collection.href && (
                          <Check className="size-2.5 text-primary-foreground" />
                        )}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-medium text-foreground">
                          {collection.displayname || collection.href}
                        </span>
                        <span className="block truncate text-[11px] text-muted-foreground">
                          {collection.href}
                        </span>
                      </span>
                      {!supportsEvents && (
                        <Chip variant="muted">No events</Chip>
                      )}
                    </button>
                  );
                })}
              </div>
            </div>
          )}

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="caldav-name">Display name</Label>
              <Input
                id="caldav-name"
                type="text"
                placeholder="My calendar"
                value={caldavName}
                onChange={(e) => setCaldavName(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <Label>Color</Label>
              <div className="flex flex-wrap gap-1.5 pt-1">
                {EVENT_COLORS.map((color) => (
                  <button
                    key={color.value}
                    type="button"
                    aria-label={color.name}
                    onClick={() => setCaldavColor(color.value)}
                    className={cn(
                      "size-5 rounded-full transition-transform",
                      color.bg,
                      caldavColor === color.value
                        ? "scale-110 ring-2 ring-ring ring-offset-2 ring-offset-background"
                        : "opacity-70 hover:opacity-100",
                    )}
                  />
                ))}
              </div>
            </div>
          </div>

          {formError && (
            <div className="rounded-md bg-destructive/10 px-3 py-2 text-xs break-words text-destructive">
              {formError}
            </div>
          )}

          <div className="flex justify-end gap-2">
            <Button variant="outline" size="sm" onClick={resetForm}>
              Cancel
            </Button>
            <Button size="sm" onClick={handleSaveCaldav} disabled={saving}>
              {saving ? "Saving..." : editingId !== null ? "Save changes" : "Connect calendar"}
            </Button>
          </div>
        </div>
      ) : (
        /* ---------------- Google panel ---------------- */
        <div className="space-y-3 rounded-lg border border-border p-4">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-foreground">Google Calendar</h3>
            <Button variant="ghost" size="icon-sm" aria-label="Cancel" onClick={resetForm}>
              <X className="size-4" />
            </Button>
          </div>
          {googleEnabled ? (
            <>
              <p className="text-sm text-muted-foreground">
                You&apos;ll be redirected to Google to sign in and grant read-only
                access to your calendars and tasks.
              </p>
              {formError && (
                <div className="rounded-md bg-destructive/10 px-3 py-2 text-xs break-words text-destructive">
                  {formError}
                </div>
              )}
              <Button onClick={handleConnectGoogle}>Sign in with Google</Button>
            </>
          ) : (
            <div className="rounded-md bg-muted/50 p-3 text-sm text-muted-foreground">
              This server doesn&apos;t have Google Calendar integration set up. An
              administrator needs to configure{" "}
              <code className="rounded bg-muted px-1 font-mono text-xs">GOOGLE_CLIENT_ID</code> and{" "}
              <code className="rounded bg-muted px-1 font-mono text-xs">GOOGLE_CLIENT_SECRET</code>{" "}
              (with{" "}
              <code className="rounded bg-muted px-1 font-mono text-xs">…/api/calendar/google/callback</code>{" "}
              as an authorized redirect URI in the Google Cloud console).
            </div>
          )}
        </div>
      )}
    </div>
  );
}
