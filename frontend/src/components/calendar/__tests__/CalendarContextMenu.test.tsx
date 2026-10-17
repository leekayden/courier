import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import type { ReactNode } from "react";
import type { CalendarContextMenuState } from "../CalendarContextMenu";

vi.mock("framer-motion", () => ({
  AnimatePresence: ({ children }: { children: ReactNode }) => <>{children}</>,
  motion: { div: ({ children, ...props }: React.HTMLAttributes<HTMLDivElement>) => <div {...props}>{children}</div> },
}));

const { mockCalendarState, mockUiState } = vi.hoisted(() => ({
  mockCalendarState: {
    setDate: vi.fn(),
  },
  mockUiState: {
    effectiveAnimationMode: "off" as "rich" | "medium" | "subtle" | "off",
  },
}));

vi.mock("@/stores/useCalendarStore", () => ({
  useCalendarStore: (selector: (s: typeof mockCalendarState) => unknown) => selector(mockCalendarState),
}));

vi.mock("@/stores/useUiStore", () => ({
  useUiStore: (selector: (s: typeof mockUiState) => unknown) => selector(mockUiState),
}));

vi.mock("@/lib/motion/AnimatedDiv", () => ({
  AnimatedDiv: ({ children, ...props }: React.HTMLAttributes<HTMLDivElement> & { variants?: unknown; initial?: unknown; animate?: unknown; exit?: unknown }) => (
    <div {...(props as React.HTMLAttributes<HTMLDivElement>)}>{children}</div>
  ),
}));

import { CalendarContextMenu } from "../CalendarContextMenu";

function makeDayState(overrides?: Partial<Extract<CalendarContextMenuState, { type: "day" }>>): CalendarContextMenuState {
  return { x: 100, y: 100, type: "day", date: new Date(2026, 2, 6), ...overrides };
}

describe("CalendarContextMenu (read-only)", () => {
  beforeEach(() => {
    mockCalendarState.setDate.mockClear();
  });

  it("renders nothing when state is null", () => {
    const { container } = render(<CalendarContextMenu state={null} onClose={vi.fn()} />);
    expect(container.querySelector(".fixed")).toBeNull();
  });

  it("renders nothing without an onAddSticker handler (no mutating actions)", () => {
    const { container } = render(<CalendarContextMenu state={makeDayState()} onClose={vi.fn()} />);
    expect(container.querySelector(".fixed")).toBeNull();
  });

  it("offers no Edit/Delete/New-event actions", () => {
    render(
      <CalendarContextMenu
        state={makeDayState()}
        onClose={vi.fn()}
        onAddSticker={vi.fn()}
      />,
    );
    expect(screen.queryByText("Edit")).toBeNull();
    expect(screen.queryByText("Delete")).toBeNull();
    expect(screen.queryByText(/New event/i)).toBeNull();
  });

  it("shows the sticker action for day cells", () => {
    const onAddSticker = vi.fn();
    render(
      <CalendarContextMenu
        state={makeDayState()}
        onClose={vi.fn()}
        onAddSticker={onAddSticker}
      />,
    );
    expect(screen.getByText(/Add sticker on Mar 6/)).toBeTruthy();
  });

  it("clicking the sticker action invokes the handler and closes", () => {
    const onAddSticker = vi.fn();
    const onClose = vi.fn();
    const state = makeDayState();
    render(
      <CalendarContextMenu
        state={state}
        onClose={onClose}
        onAddSticker={onAddSticker}
      />,
    );
    fireEvent.click(screen.getByText(/Add sticker/));
    expect(onAddSticker).toHaveBeenCalledWith(state.date);
    expect(mockCalendarState.setDate).toHaveBeenCalledWith(state.date);
    expect(onClose).toHaveBeenCalled();
  });
});
