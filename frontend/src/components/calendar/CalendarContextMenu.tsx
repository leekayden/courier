"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence } from "framer-motion";
import { Smile } from "lucide-react";
import { AnimatedDiv } from "@/lib/motion/AnimatedDiv";
import { createScaleFadeVariants } from "@/lib/motion/variants";
import { useUiStore } from "@/stores/useUiStore";
import { useCalendarStore } from "@/stores/useCalendarStore";

/**
 * Context menu state. Null = closed.
 *
 * The calendar is read-only, so the only entry is the (feature-flagged)
 * sticker action on day cells.
 */
export type CalendarContextMenuState =
  | { x: number; y: number; type: "day"; date: Date };

/** Props for CalendarContextMenu. */
type CalendarContextMenuProps = {
  state: CalendarContextMenuState | null;
  onClose: () => void;
  /** When provided, shows "Add sticker" on day cells. */
  onAddSticker?: (date: Date) => void;
};

const MENU_WIDTH = 188;
const MENU_MAX_HEIGHT = 160;
const MARGIN = 8;

export function CalendarContextMenu({ state, onClose, onAddSticker }: CalendarContextMenuProps) {
  const setDate = useCalendarStore((s) => s.setDate);
  const effectiveAnimationMode = useUiStore((s) => s.effectiveAnimationMode);
  const motionProps = useMemo(() => createScaleFadeVariants(effectiveAnimationMode), [effectiveAnimationMode]);
  const ref = useRef<HTMLDivElement>(null);

  const handleClose = useCallback(() => {
    onClose();
  }, [onClose]);

  useEffect(() => {
    if (!state) return;
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        handleClose();
      }
    }
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) handleClose();
    }
    document.addEventListener("keydown", handleKey);
    document.addEventListener("mousedown", handleClick);
    return () => {
      document.removeEventListener("keydown", handleKey);
      document.removeEventListener("mousedown", handleClick);
    };
  }, [state, handleClose]);

  const style = useMemo(() => {
    if (!state) return {};
    let left = state.x + MARGIN;
    let top = state.y + MARGIN;
    if (left + MENU_WIDTH > window.innerWidth - MARGIN) left = state.x - MENU_WIDTH - MARGIN;
    if (top + MENU_MAX_HEIGHT > window.innerHeight - MARGIN) top = state.y - MENU_MAX_HEIGHT - MARGIN;
    return { left, top, width: MENU_WIDTH };
  }, [state]);

  if (typeof document === "undefined") return null;
  if (!state || !onAddSticker) return null;

  return createPortal(
    <AnimatePresence>
      {state && (
        <AnimatedDiv
          ref={ref}
          variants={motionProps}
          initial="initial"
          animate="animate"
          exit="exit"
          className="fixed z-[70] overflow-hidden rounded-lg border border-border bg-background py-1 shadow-xl"
          style={style}
        >
          {state.type === "day" && (
            <button
              type="button"
              onClick={() => {
                setDate(state.date);
                onAddSticker(state.date);
                handleClose();
              }}
              className="flex w-full items-center gap-2.5 px-3 py-1.5 text-sm text-foreground hover:bg-accent"
            >
              <Smile className="size-3.5 shrink-0 text-muted-foreground" />
              <span className="truncate">
                Add sticker on {state.date.toLocaleDateString("en-US", { month: "short", day: "numeric" })}
              </span>
            </button>
          )}
        </AnimatedDiv>
      )}
    </AnimatePresence>,
    document.body,
  );
}
