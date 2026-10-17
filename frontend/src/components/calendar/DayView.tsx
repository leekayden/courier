"use client";

import { useMemo, useRef, useEffect, useState } from "react";
import { Circle, CircleDashed } from "lucide-react";
import { useCalendarStore } from "@/stores/useCalendarStore";
import { useCalendarEvents } from "@/hooks/useCalendar";
import {
  getDayViewRange,
  getHours,
  layoutEvents,
  getOverflowGroups,
  isToday,
  formatTime,
  getEventColorClasses,
} from "./calendarUtils";
import { EventListPopover, type EventListPopoverState } from "./EventListPopover";
import { EventChip } from "./EventChip";
import { cn } from "@/lib/utils";
import type { CalendarEvent } from "@/types/calendar";

type DayViewProps = {
  timeFormat: string;
};

export function DayView({ timeFormat }: DayViewProps) {
  const selectedDate = useCalendarStore((s) => s.selectedDate);
  const setDate = useCalendarStore((s) => s.setDate);
  const selectEvent = useCalendarStore((s) => s.selectEvent);
  const scrollRef = useRef<HTMLDivElement>(null);

  const range = useMemo(
    () => getDayViewRange(selectedDate),
    [selectedDate],
  );
  const { data } = useCalendarEvents(range.start, range.end);
  const events = useMemo(() => data?.events ?? [], [data]);
  const todos = useMemo(() => data?.todos ?? [], [data]);

  const hours = getHours();

  // Scroll to 8am on mount
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = 8 * 60;
    }
  }, []);

  const { allDayEvents, timedEvents } = useMemo(() => {
    const allDay: CalendarEvent[] = [];
    const timed: CalendarEvent[] = [];

    for (const event of events) {
      if (event.all_day) {
        allDay.push(event);
      } else {
        timed.push(event);
      }
    }

    return { allDayEvents: allDay, timedEvents: timed };
  }, [events]);

  // Tasks for the selected day: due today first, then overdue, then undated.
  // Completed tasks collapse into a single line.
  const { openTasks, completedCount } = useMemo(() => {
    const dayStart = new Date(selectedDate);
    dayStart.setHours(0, 0, 0, 0);
    const dayEnd = new Date(selectedDate);
    dayEnd.setHours(23, 59, 59, 999);

    const dueToday: CalendarEvent[] = [];
    const overdue: CalendarEvent[] = [];
    const undated: CalendarEvent[] = [];
    let completed = 0;

    for (const todo of todos) {
      if (todo.status === "completed" || todo.completed_at) {
        completed += 1;
        continue;
      }
      if (!todo.end_time) {
        undated.push(todo);
        continue;
      }
      const due = new Date(todo.end_time);
      if (Number.isNaN(due.getTime()) || due > dayEnd) continue;
      if (due >= dayStart) {
        dueToday.push(todo);
      } else {
        overdue.push(todo);
      }
    }

    const byDue = (a: CalendarEvent, b: CalendarEvent) =>
      (a.end_time || "").localeCompare(b.end_time || "");

    return {
      openTasks: [...dueToday.sort(byDue), ...overdue.sort(byDue), ...undated],
      completedCount: completed,
    };
  }, [todos, selectedDate]);

  const handleTimeSlotClick = (hour: number) => {
    const d = new Date(selectedDate);
    d.setHours(hour, 0, 0, 0);
    setDate(d);
  };

  const handleEventClick = (e: React.MouseEvent, eventId: string) => {
    e.stopPropagation();
    selectEvent(eventId);
  };

  const DAY_COL_CAP = 5;
  const [overflowPopover, setOverflowPopover] = useState<EventListPopoverState | null>(null);

  const [currentMinutes, setCurrentMinutes] = useState(() => {
    const now = new Date();
    return now.getHours() * 60 + now.getMinutes();
  });
  useEffect(() => {
    const id = setInterval(() => {
      const now = new Date();
      setCurrentMinutes(now.getHours() * 60 + now.getMinutes());
    }, 60_000);
    return () => clearInterval(id);
  }, []);

  const showIndicator = isToday(selectedDate);

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* All-day events */}
      {allDayEvents.length > 0 && (
        <div className="flex border-b border-border">
          <div className="w-[60px] shrink-0 border-r border-border px-1 py-1 text-[10px] text-muted-foreground">
            All day
          </div>
          <div className="flex-1 p-1 space-y-0.5">
            {allDayEvents.map((ev) => {
              const colors = getEventColorClasses(ev.color);
              return (
                <button
                  key={ev.id}
                  type="button"
                  onClick={(e) => handleEventClick(e, ev.id)}
                  className={cn(
                    "block w-full truncate rounded px-2 py-1 text-left text-xs",
                    colors.bg,
                    colors.text,
                  )}
                >
                  {ev.title}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Time grid */}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        <div className="relative flex">
          {/* Time labels */}
          <div className="w-[60px] shrink-0 border-r border-border">
            {hours.map((hour) => (
              <div
                key={hour}
                className="relative h-[60px] border-b border-border pr-2 text-right"
              >
                <span className="absolute -top-2 right-2 text-[10px] text-muted-foreground">
                  {formatTime(
                    `2025-01-01T${String(hour).padStart(2, "0")}:00:00`,
                    timeFormat,
                  )}
                </span>
              </div>
            ))}
          </div>

          {/* Event area */}
          <div className="relative flex-1">
            {/* Hour slots */}
            {hours.map((hour) => (
              <div
                key={hour}
                onClick={() => handleTimeSlotClick(hour)}
                className="h-[60px] cursor-pointer border-b border-border transition-colors hover:bg-accent/30"
              />
            ))}

            {/* Current time indicator */}
            {showIndicator && (
              <div
                className="pointer-events-none absolute left-0 right-0 z-10 border-t-2 border-red-500"
                style={{ top: `${currentMinutes}px` }}
              >
                <div className="absolute -left-1 -top-1.5 size-3 rounded-full bg-red-500" />
              </div>
            )}

            {/* Events */}
            {(() => {
              const laid = layoutEvents(timedEvents, selectedDate);
              const overflowGroups = getOverflowGroups(laid, DAY_COL_CAP);
              return (
                <>
                  {laid.filter((ev) => ev.column < DAY_COL_CAP).map((event) => {
                    const colors = getEventColorClasses(event.color);
                    const effectiveCols = Math.min(event.totalColumns, DAY_COL_CAP);
                    const colWidth = 100 / effectiveCols;
                    return (
                      <EventChip
                        key={event.id}
                        event={event}
                        onClick={(e) => handleEventClick(e, event.id)}
                        className={cn(
                          "absolute overflow-hidden rounded px-2 py-1 text-left text-xs leading-tight",
                          colors.bg,
                          colors.text,
                        )}
                        style={{
                          top: `${event.top}px`,
                          height: `${event.height}px`,
                          left: `calc(${event.column * colWidth}% + 2px)`,
                          right: `calc(${(effectiveCols - event.column - 1) * colWidth}% + 2px)`,
                        }}
                        title={event.title}
                      >
                        <div className="truncate font-medium">{event.title}</div>
                        <div className="opacity-80">
                          {formatTime(event.start_time, timeFormat)} -{" "}
                          {formatTime(event.end_time, timeFormat)}
                        </div>
                        {event.location && (
                          <div className="mt-0.5 truncate opacity-70">{event.location}</div>
                        )}
                      </EventChip>
                    );
                  })}
                  {overflowGroups.map((group, i) => (
                    <button
                      key={`overflow-${i}`}
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        setOverflowPopover({
                          anchor: e.currentTarget.getBoundingClientRect(),
                          events: group.all as unknown as CalendarEvent[],
                          title: `${group.all.length} events`,
                        });
                      }}
                      className="absolute right-1 z-10 rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
                      style={{ top: `${group.top}px` }}
                    >
                      +{group.hidden.length}
                    </button>
                  ))}
                </>
              );
            })()}
          </div>
        </div>
      </div>

      {/* Tasks (VTODOs / Google Tasks) - read-only */}
      <div className="max-h-[30%] shrink-0 overflow-y-auto border-t border-border" data-testid="day-view-tasks">
        <div className="flex items-center justify-between px-3 py-1.5">
          <span className="text-xs font-semibold text-foreground">
            Tasks
          </span>
          {completedCount > 0 && (
            <span className="text-[10px] text-muted-foreground">
              {completedCount} completed
            </span>
          )}
        </div>
        {openTasks.length === 0 ? (
          <div className="px-3 pb-2 text-xs text-muted-foreground">
            No open tasks.
          </div>
        ) : (
          <div className="pb-1">
            {openTasks.map((todo) => {
              const isOverdue =
                !!todo.end_time
                && new Date(todo.end_time) < new Date()
                && todo.status !== "completed"
                && !todo.completed_at;
              return (
                <button
                  key={todo.id}
                  type="button"
                  onClick={(e) => handleEventClick(e, todo.id)}
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-accent"
                >
                  <Circle className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate text-xs text-foreground">
                    {todo.title}
                  </span>
                  {todo.end_time ? (
                    <span
                      className={cn(
                        "shrink-0 text-[10px]",
                        isOverdue ? "text-destructive" : "text-muted-foreground",
                      )}
                    >
                      {new Date(todo.end_time).toLocaleDateString("en-US", {
                        month: "short",
                        day: "numeric",
                      })}{" "}
                      {formatTime(todo.end_time, timeFormat)}
                    </span>
                  ) : (
                    <CircleDashed className="size-3 shrink-0 text-muted-foreground/40" />
                  )}
                </button>
              );
            })}
          </div>
        )}
      </div>

      <EventListPopover
        state={overflowPopover}
        onClose={() => setOverflowPopover(null)}
        timeFormat={timeFormat}
      />
    </div>
  );
}
