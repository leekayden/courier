"use client";

import { useEffect, useMemo, useState } from "react";
import { X, ChevronUp, ChevronDown, Trash2, Plus } from "lucide-react";

type Align = "left" | "center" | "right";

type Block =
  | { id: string; type: "heading"; text: string; level: 1 | 2 | 3; align: Align; color: string }
  | { id: string; type: "text"; text: string; align: Align; color: string }
  | { id: string; type: "image"; src: string; href: string; width: number }
  | { id: string; type: "button"; text: string; href: string; bg: string; color: string }
  | { id: string; type: "divider"; color: string }
  | { type: "spacer"; id: string; height: number }
  | { id: string; type: "columns2"; left: string; right: string };

const PALETTE: { type: Block["type"]; label: string }[] = [
  { type: "heading", label: "Heading" },
  { type: "text", label: "Text" },
  { type: "image", label: "Image" },
  { type: "button", label: "Button" },
  { type: "divider", label: "Divider" },
  { type: "spacer", label: "Spacer" },
  { type: "columns2", label: "2 columns" },
];

let idCounter = 0;
const newId = () => `b${Date.now().toString(36)}${idCounter++}`;
const newBlock = (type: Block["type"]): Block => {
  switch (type) {
    case "heading": return { id: newId(), type, text: "Your heading", level: 2, align: "center", color: "#111827" };
    case "text": return { id: newId(), type, text: "Write your message here.", align: "left", color: "#374151" };
    case "image": return { id: newId(), type, src: "", href: "", width: 100 };
    case "button": return { id: newId(), type, text: "Click here", href: "https://", bg: "#4f46e5", color: "#ffffff" };
    case "divider": return { id: newId(), type, color: "#e5e7eb" };
    case "spacer": return { id: newId(), type, height: 24 };
    case "columns2": return { id: newId(), type, left: "Left column", right: "Right column" };
  }
};

const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const nl2br = (s: string) => esc(s).replace(/\n/g, "<br />");
const alignStyle = (a: Align) => `text-align:${a};`;

export function blocksToEmailHtml(blocks: Block[], bg: string): string {
  const rows = blocks
    .map((b) => {
      switch (b.type) {
        case "heading":
          return `<tr><td style="padding:8px 24px;${alignStyle(b.align)}"><h${b.level + 1} style="margin:0;font-family:Arial,sans-serif;font-size:${b.level === 1 ? 28 : b.level === 2 ? 22 : 18}px;color:${b.color};${alignStyle(b.align)}">${nl2br(b.text)}</h${b.level + 1}></td></tr>`;
        case "text":
          return `<tr><td style="padding:8px 24px;${alignStyle(b.align)}"><div style="font-family:Arial,sans-serif;font-size:15px;line-height:1.6;color:${b.color};${alignStyle(b.align)}">${nl2br(b.text)}</div></td></tr>`;
        case "image":
          return `<tr><td style="padding:8px 24px;${alignStyle("center")}">${b.href ? `<a href="${esc(b.href)}">` : ""}<img src="${esc(b.src)}" alt="" width="${b.width}%" style="width:${b.width}%;max-width:100%;height:auto;display:block;margin:0 auto;border:none" />${b.href ? "</a>" : ""}</td></tr>`;
        case "button":
          return `<tr><td style="padding:12px 24px;${alignStyle("center")}"><a href="${esc(b.href)}" style="display:inline-block;background:${b.bg};color:${b.color};font-family:Arial,sans-serif;font-size:15px;font-weight:bold;padding:12px 28px;border-radius:6px;text-decoration:none">${esc(b.text)}</a></td></tr>`;
        case "divider":
          return `<tr><td style="padding:8px 24px"><div style="border-top:1px solid ${b.color};line-height:0;font-size:0">&nbsp;</div></td></tr>`;
        case "spacer":
          return `<tr><td style="height:${b.height}px;line-height:${b.height}px;font-size:0">&nbsp;</td></tr>`;
        case "columns2":
          return `<tr><td style="padding:8px 24px"><table role="presentation" width="100%" cellpadding="0" cellspacing="0"><tr>
<td width="50%" style="vertical-align:top;padding-right:8px"><div style="font-family:Arial,sans-serif;font-size:14px;line-height:1.6;color:#374151">${nl2br(b.left)}</div></td>
<td width="50%" style="vertical-align:top;padding-left:8px"><div style="font-family:Arial,sans-serif;font-size:14px;line-height:1.6;color:#374151">${nl2br(b.right)}</div></td>
</tr></table></td></tr>`;
      }
    })
    .join("\n");

  return `<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:${bg};padding:24px 0;"><tr><td align="center">
<table role="presentation" width="600" cellpadding="0" cellspacing="0" style="max-width:600px;width:100%;background:#ffffff;border-radius:8px;overflow:hidden;">
${rows}
</table>
</td></tr></table>`;
}

export type EmailBuilderDialogProps = {
  open: boolean;
  onClose: () => void;
  onInsert: (html: string) => void;
  onReplace: (html: string) => void;
};

export function EmailBuilderDialog({ open, onClose, onInsert, onReplace }: EmailBuilderDialogProps) {
  const [blocks, setBlocks] = useState<Block[]>([
    newBlock("heading"),
    newBlock("text"),
    newBlock("button"),
  ]);
  const [bg, setBg] = useState("#f4f4f7");
  const [mobile, setMobile] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);

  const html = useMemo(() => blocksToEmailHtml(blocks, bg), [blocks, bg]);

  const move = (idx: number, dir: -1 | 1) => {
    const to = idx + dir;
    if (to < 0 || to >= blocks.length) return;
    const next = [...blocks];
    [next[idx], next[to]] = [next[to], next[idx]];
    setBlocks(next);
  };
  const update = (id: string, patch: Partial<Block>) =>
    setBlocks((bs) => bs.map((b) => (b.id === id ? ({ ...b, ...patch } as Block) : b)));
  const remove = (id: string) => setBlocks((bs) => bs.filter((b) => b.id !== id));
  const add = (type: Block["type"]) => {
    const b = newBlock(type);
    setBlocks((bs) => [...bs, b]);
    setSelected(b.id);
  };

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  const field = "w-full rounded-md border border-border bg-transparent px-2 py-1.5 text-xs outline-none focus:ring-1 focus:ring-ring";
  const label = "text-[10px] uppercase tracking-wide text-muted-foreground";

  return (
    <div className="fixed inset-0 z-[120] flex items-center justify-center bg-black/70 p-4">
      <div className="flex h-full w-full max-w-6xl flex-col rounded-xl border border-border bg-background shadow-2xl">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-sm font-semibold">Email builder</h2>
          <div className="flex items-center gap-2">
            <button onClick={() => setMobile(!mobile)} className="rounded-md border border-border px-2 py-1 text-xs hover:bg-accent">
              {mobile ? "Desktop preview" : "Mobile preview"}
            </button>
            <button onClick={onClose} className="rounded-md p-1 hover:bg-accent" title="Close">
              <X className="size-4" />
            </button>
          </div>
        </div>

        <div className="flex min-h-0 flex-1">
          {/* Editor column */}
          <div className="w-[400px] shrink-0 overflow-y-auto border-r border-border p-3">
            <div className="mb-3">
              <span className={label}>Page background</span>
              <div className="mt-1 flex items-center gap-2">
                <input type="color" value={bg} onChange={(e) => setBg(e.target.value)} className="h-7 w-10 rounded border border-border" />
                <span className="text-xs text-muted-foreground">{bg}</span>
              </div>
            </div>

            <div className="space-y-2">
              {blocks.map((b, idx) => (
                <div key={b.id} className={`rounded-lg border p-2.5 ${selected === b.id ? "border-ring" : "border-border"}`} onClick={() => setSelected(b.id)}>
                  <div className="mb-2 flex items-center justify-between">
                    <span className="text-xs font-semibold capitalize text-muted-foreground">{b.type === "columns2" ? "2 columns" : b.type}</span>
                    <div className="flex items-center gap-1">
                      <button onClick={() => move(idx, -1)} className="rounded p-0.5 hover:bg-accent" title="Move up"><ChevronUp className="size-3.5" /></button>
                      <button onClick={() => move(idx, 1)} className="rounded p-0.5 hover:bg-accent" title="Move down"><ChevronDown className="size-3.5" /></button>
                      <button onClick={() => remove(b.id)} className="rounded p-0.5 text-red-400 hover:bg-accent" title="Delete"><Trash2 className="size-3.5" /></button>
                    </div>
                  </div>

                  {b.type === "heading" && (
                    <div className="space-y-1.5">
                      <textarea value={b.text} onChange={(e) => update(b.id, { text: e.target.value } as Partial<Block>)} className={field} rows={2} />
                      <div className="flex items-center gap-2">
                        <select value={b.level} onChange={(e) => update(b.id, { level: Number(e.target.value) } as Partial<Block>)} className={field}>
                          <option value={1}>H1</option><option value={2}>H2</option><option value={3}>H3</option>
                        </select>
                        <select value={b.align} onChange={(e) => update(b.id, { align: e.target.value } as Partial<Block>)} className={field}>
                          <option value="left">Left</option><option value="center">Center</option><option value="right">Right</option>
                        </select>
                        <input type="color" value={b.color} onChange={(e) => update(b.id, { color: e.target.value } as Partial<Block>)} className="h-7 w-9 rounded border border-border" />
                      </div>
                    </div>
                  )}
                  {b.type === "text" && (
                    <div className="space-y-1.5">
                      <textarea value={b.text} onChange={(e) => update(b.id, { text: e.target.value } as Partial<Block>)} className={field} rows={4} />
                      <div className="flex items-center gap-2">
                        <select value={b.align} onChange={(e) => update(b.id, { align: e.target.value } as Partial<Block>)} className={field}>
                          <option value="left">Left</option><option value="center">Center</option><option value="right">Right</option>
                        </select>
                        <input type="color" value={b.color} onChange={(e) => update(b.id, { color: e.target.value } as Partial<Block>)} className="h-7 w-9 rounded border border-border" />
                      </div>
                    </div>
                  )}
                  {b.type === "image" && (
                    <div className="space-y-1.5">
                      <input value={b.src} onChange={(e) => update(b.id, { src: e.target.value } as Partial<Block>)} placeholder="Image URL" className={field} />
                      <input value={b.href} onChange={(e) => update(b.id, { href: e.target.value } as Partial<Block>)} placeholder="Link (optional)" className={field} />
                      <input type="number" min={10} max={100} value={b.width} onChange={(e) => update(b.id, { width: Number(e.target.value) } as Partial<Block>)} className={field} placeholder="width %" />
                    </div>
                  )}
                  {b.type === "button" && (
                    <div className="space-y-1.5">
                      <input value={b.text} onChange={(e) => update(b.id, { text: e.target.value } as Partial<Block>)} placeholder="Button text" className={field} />
                      <input value={b.href} onChange={(e) => update(b.id, { href: e.target.value } as Partial<Block>)} placeholder="Link URL" className={field} />
                      <div className="flex items-center gap-2">
                        <input type="color" value={b.bg} onChange={(e) => update(b.id, { bg: e.target.value } as Partial<Block>)} className="h-7 w-9 rounded border border-border" title="Background" />
                        <input type="color" value={b.color} onChange={(e) => update(b.id, { color: e.target.value } as Partial<Block>)} className="h-7 w-9 rounded border border-border" title="Text color" />
                      </div>
                    </div>
                  )}
                  {b.type === "divider" && (
                    <input type="color" value={b.color} onChange={(e) => update(b.id, { color: e.target.value } as Partial<Block>)} className="h-7 w-9 rounded border border-border" title="Divider color" />
                  )}
                  {b.type === "spacer" && (
                    <input type="number" min={4} max={200} value={b.height} onChange={(e) => update(b.id, { height: Number(e.target.value) } as Partial<Block>)} className={field} />
                  )}
                  {b.type === "columns2" && (
                    <div className="space-y-1.5">
                      <textarea value={b.left} onChange={(e) => update(b.id, { left: e.target.value } as Partial<Block>)} className={field} rows={3} placeholder="Left column" />
                      <textarea value={b.right} onChange={(e) => update(b.id, { right: e.target.value } as Partial<Block>)} className={field} rows={3} placeholder="Right column" />
                    </div>
                  )}
                </div>
              ))}
            </div>

            <div className="mt-3 flex flex-wrap gap-1.5 border-t border-border pt-3">
              {PALETTE.map((p) => (
                <button key={p.type} onClick={() => add(p.type)} className="flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent">
                  <Plus className="size-3" /> {p.label}
                </button>
              ))}
            </div>
          </div>

          {/* Preview column */}
          <div className="flex min-w-0 flex-1 flex-col bg-accent/30 p-4">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-xs text-muted-foreground">Live preview</span>
            </div>
            <div className="flex-1 overflow-auto rounded-lg border border-border bg-white" style={{ maxWidth: mobile ? 375 : "100%", margin: "0 auto", width: "100%" }}>
              <iframe
                title="Email preview"
                srcDoc={`<!DOCTYPE html><html><body style="margin:0;padding:0;">${html}</body></html>`}
                className="h-full min-h-[500px] w-full border-0"
                style={{ height: "100%", minHeight: 500 }}
              />
            </div>
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3">
          <button onClick={() => { onReplace(html); }} className="rounded-md border border-border px-3 py-2 text-sm hover:bg-accent">
            Replace email body
          </button>
          <button
            onClick={() => { onInsert(html); }}
            className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
          >
            Append to email
          </button>
        </div>
      </div>
    </div>
  );
}
