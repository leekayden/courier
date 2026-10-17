"use client";

import { useCallback, useEffect, useState } from "react";
import { useAuthStore } from "@/stores/useAuthStore";
import { fetchAccounts } from "@/lib/api";

// Accounts allowed to open the mail admin.
const ADMIN_EMAILS = [
  "kayden@cloudservetechcentral.com",
  "admin@cloudservetechcentral.com",
];

const API = "/mailadmin-api/api.php";

type MailUser = { id: number; email: string };
type MailAlias = { id: number; source: string; destination: string };
type MailDomain = { id: number; name: string };
type State = {
  ok: boolean;
  csrf: string;
  users: MailUser[];
  aliases: MailAlias[];
  domains: MailDomain[];
};

const jsonHeaders = (csrf: string) => ({
  "Content-Type": "application/json",
  "X-Requested-With": "XMLHttpRequest",
  "X-CSRF-Token": csrf,
});

export default function AdminPage() {
  const accounts = useAuthStore((s) => s.accounts);
  const activeAccountId = useAuthStore((s) => s.activeAccountId);
  const activeEmail =
    accounts.find((a) => a.id === activeAccountId)?.email ?? "";
  const isAdmin = ADMIN_EMAILS.includes(activeEmail);

  const [authed, setAuthed] = useState<boolean | null>(null);
  const [state, setState] = useState<State | null>(null);
  const [pw, setPw] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<"users" | "aliases">("users");

  const setAccounts = useAuthStore((s) => s.setAccounts);
  const [bootstrapped, setBootstrapped] = useState(false);

  // /admin sits outside the authenticated app layout, so on a cold load the
  // auth store is empty - bootstrap the signed-in accounts from the session.
  useEffect(() => {
    if (accounts.length > 0) {
      setBootstrapped(true);
      return;
    }
    fetchAccounts()
      .then((res) => setAccounts(res.accounts))
      .catch(() => {})
      .finally(() => setBootstrapped(true));
  }, [accounts.length, setAccounts]);

  const load = useCallback(async () => {
    const res = await fetch(`${API}?action=state`, {
      headers: { "X-Requested-With": "XMLHttpRequest" },
    });
    if (res.status === 401) {
      sessionStorage.removeItem("mailadminAuth");
      setAuthed(false);
      return;
    }
    const data = (await res.json()) as State;
    setState(data);
    sessionStorage.setItem("mailadminCsrf", data.csrf);
    setAuthed(true);
  }, []);

  useEffect(() => {
    if (!isAdmin) return;
    if (sessionStorage.getItem("mailadminAuth") === "1") {
      load();
    } else {
      setAuthed(false);
    }
  }, [isAdmin, load]);

  const login = async () => {
    setErr("");
    setBusy(true);
    try {
      const res = await fetch(`${API}?action=login`, {
        method: "POST",
        headers: jsonHeaders(""),
        body: JSON.stringify({ password: pw }),
      });
      if (!res.ok) {
        const d = await res.json().catch(() => ({ error: "login failed" }));
        setErr(d.error ?? "login failed");
        return;
      }
      sessionStorage.setItem("mailadminAuth", "1");
      await load();
    } finally {
      setBusy(false);
    }
  };

  const act = useCallback(
    async (action: string, payload: Record<string, unknown>) => {
      setErr("");
      setBusy(true);
      try {
        const csrf = sessionStorage.getItem("mailadminCsrf") ?? "";
        const res = await fetch(`${API}?action=${action}`, {
          method: "POST",
          headers: jsonHeaders(csrf),
          body: JSON.stringify(payload),
        });
        const data = await res.json().catch(() => ({}));
        if (!res.ok) {
          setErr(data.error ?? "request failed");
          return false;
        }
        await load();
        return true;
      } finally {
        setBusy(false);
      }
    },
    [load],
  );

  if (accounts.length === 0 && !bootstrapped) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-background">
        <p className="text-sm text-muted-foreground">Checking session…</p>
      </main>
    );
  }

  if (accounts.length === 0 && bootstrapped) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-background p-6">
        <div className="rounded-lg border border-border bg-card p-8 text-center">
          <h1 className="text-lg font-semibold">No signed-in account</h1>
          <p className="mt-2 text-sm text-muted-foreground">
            Open the webmail, sign in, and come back to this page.
          </p>
        </div>
      </main>
    );
  }

  if (!isAdmin) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-background p-6">
        <div className="rounded-lg border border-border bg-card p-8 text-center">
          <h1 className="text-lg font-semibold">Not authorized</h1>
          <p className="mt-2 text-sm text-muted-foreground">
            Sign in as an administrator account to use the mail admin.
          </p>
        </div>
      </main>
    );
  }

  if (authed === null || (authed && !state)) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-background">
        <p className="text-sm text-muted-foreground">Loading…</p>
      </main>
    );
  }

  if (!authed) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-background p-6">
        <div className="w-full max-w-sm rounded-lg border border-border bg-card p-8">
          <h1 className="text-lg font-semibold">Mail admin</h1>
          <p className="mt-1 mb-4 text-sm text-muted-foreground">
            Enter the mail admin password.
          </p>
          {err && <p className="mb-3 text-sm text-red-500">{err}</p>}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              login();
            }}
          >
            <input
              type="password"
              value={pw}
              onChange={(e) => setPw(e.target.value)}
              placeholder="Admin password"
              className="w-full rounded-md border border-border bg-transparent px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-ring"
            />
            <button
              type="submit"
              disabled={busy}
              className="mt-3 w-full rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              {busy ? "Checking…" : "Sign in"}
            </button>
          </form>
        </div>
      </main>
    );
  }

  return (
    <main className="min-h-screen bg-background p-6 text-foreground sm:p-10">
      <div className="mx-auto max-w-3xl">
        <div className="flex items-center justify-between">
          <h1 className="text-xl font-semibold">Mail admin</h1>
          <button
            onClick={() => {
              sessionStorage.removeItem("mailadminAuth");
              setAuthed(false);
              setState(null);
            }}
            className="rounded-md border border-border px-3 py-1.5 text-xs hover:bg-accent"
          >
            Lock
          </button>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          Manage mailboxes and aliases for cloudservetechcentral.com. Changes
          apply immediately to Postfix and Dovecot.
        </p>

        {err && (
          <div className="mt-4 rounded-md border border-red-900/50 bg-red-950/40 p-3 text-sm text-red-400">
            {err}
          </div>
        )}

        <div className="mt-6 flex gap-2">
          {(["users", "aliases"] as const).map((t) => (
            <button
              key={t}
              onClick={() => setTab(t)}
              className={`rounded-md px-3 py-1.5 text-sm capitalize ${
                tab === t
                  ? "bg-primary text-primary-foreground"
                  : "border border-border hover:bg-accent"
              }`}
            >
              {t}
            </button>
          ))}
        </div>

        {tab === "users" && state && (
          <>
            <h2 className="mt-8 mb-3 text-sm font-semibold text-muted-foreground">
              Mailboxes
            </h2>
            <div className="divide-y divide-border rounded-lg border border-border">
              {state.users.map((u) => (
                <div
                  key={u.id}
                  className="flex flex-wrap items-center justify-between gap-2 p-3"
                >
                  <span className="font-mono text-sm">{u.email}</span>
                  <div className="flex items-center gap-2">
                    <ResetForm
                      onSubmit={(password) =>
                        act("reset_pw", { id: u.id, password })
                      }
                      busy={busy}
                    />
                    {u.email !== "kayden@cloudservetechcentral.com" && (
                      <button
                        disabled={busy}
                        onClick={() => {
                          if (confirm(`Delete mailbox ${u.email}?`))
                            act("del_user", { id: u.id });
                        }}
                        className="rounded-md border border-red-900/50 px-2 py-1 text-xs text-red-400 hover:bg-red-950/40"
                      >
                        Delete
                      </button>
                    )}
                  </div>
                </div>
              ))}
            </div>
            <AddUserForm domains={state.domains} onSubmit={act} busy={busy} />
          </>
        )}

        {tab === "aliases" && state && (
          <>
            <h2 className="mt-8 mb-3 text-sm font-semibold text-muted-foreground">
              Aliases
            </h2>
            <div className="divide-y divide-border rounded-lg border border-border">
              {state.aliases.length === 0 && (
                <p className="p-3 text-sm text-muted-foreground">No aliases.</p>
              )}
              {state.aliases.map((a) => (
                <div
                  key={a.id}
                  className="flex flex-wrap items-center justify-between gap-2 p-3"
                >
                  <span className="font-mono text-sm">
                    {a.source} → {a.destination}
                  </span>
                  <button
                    disabled={busy}
                    onClick={() => {
                      if (confirm(`Delete alias ${a.source}?`))
                        act("del_alias", { id: a.id });
                    }}
                    className="rounded-md border border-red-900/50 px-2 py-1 text-xs text-red-400 hover:bg-red-950/40"
                  >
                    Delete
                  </button>
                </div>
              ))}
            </div>
            <AddAliasForm onSubmit={act} busy={busy} />
          </>
        )}
      </div>
    </main>
  );
}

function AddUserForm({
  domains,
  onSubmit,
  busy,
}: {
  domains: MailDomain[];
  onSubmit: (
    action: string,
    payload: Record<string, unknown>,
  ) => Promise<boolean>;
  busy: boolean;
}) {
  const [local, setLocal] = useState("");
  const [domainId, setDomainId] = useState(domains[0]?.id ?? 1);
  const [password, setPassword] = useState("");
  return (
    <form
      className="mt-4 flex flex-wrap items-center gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit("add_user", { local, domain_id: domainId, password }).then(
          (ok) => {
            if (ok) {
              setLocal("");
              setPassword("");
            }
          },
        );
      }}
    >
      <input
        value={local}
        onChange={(e) => setLocal(e.target.value)}
        placeholder="local part"
        className="w-40 rounded-md border border-border bg-transparent px-3 py-2 text-sm"
        required
      />
      <span className="text-sm text-muted-foreground">@</span>
      <select
        value={domainId}
        onChange={(e) => setDomainId(Number(e.target.value))}
        className="rounded-md border border-border bg-background px-2 py-2 text-sm"
      >
        {domains.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name}
          </option>
        ))}
      </select>
      <input
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        placeholder="password (min 10)"
        className="w-44 rounded-md border border-border bg-transparent px-3 py-2 text-sm"
        required
      />
      <button
        disabled={busy}
        className="rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
      >
        Create mailbox
      </button>
    </form>
  );
}

function AddAliasForm({
  onSubmit,
  busy,
}: {
  onSubmit: (action: string, payload: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
}) {
  const [source, setSource] = useState("");
  const [destination, setDestination] = useState("");
  return (
    <form
      className="mt-4 flex flex-wrap items-center gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit("add_alias", { source, destination }).then((ok) => {
          if (ok) {
            setSource("");
            setDestination("");
          }
        });
      }}
    >
      <input
        value={source}
        onChange={(e) => setSource(e.target.value)}
        placeholder="source@cloudservetechcentral.com"
        className="w-64 rounded-md border border-border bg-transparent px-3 py-2 text-sm"
        required
      />
      <span className="text-sm text-muted-foreground">→</span>
      <input
        value={destination}
        onChange={(e) => setDestination(e.target.value)}
        placeholder="destination@anywhere"
        className="w-64 rounded-md border border-border bg-transparent px-3 py-2 text-sm"
        required
      />
      <button
        disabled={busy}
        className="rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
      >
        Create alias
      </button>
    </form>
  );
}

function ResetForm({
  onSubmit,
  busy,
}: {
  onSubmit: (payload: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [password, setPassword] = useState("");
  if (!open)
    return (
      <button
        onClick={() => setOpen(true)}
        className="rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
      >
        Reset password
      </button>
    );
  return (
    <form
      className="flex items-center gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit({ password }).then((ok) => {
          if (ok) {
            setPassword("");
            setOpen(false);
          }
        });
      }}
    >
      <input
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        placeholder="new password"
        className="w-36 rounded-md border border-border bg-transparent px-2 py-1 text-xs"
        required
      />
      <button
        disabled={busy}
        className="rounded-md bg-primary px-2 py-1 text-xs text-primary-foreground"
      >
        Save
      </button>
      <button
        type="button"
        onClick={() => setOpen(false)}
        className="text-xs text-muted-foreground"
      >
        Cancel
      </button>
    </form>
  );
}
