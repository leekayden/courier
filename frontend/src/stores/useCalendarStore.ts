"use client";

import { create } from "zustand";

type CalendarState = {
  selectedDate: Date;
  viewMode: "month" | "week" | "day";
  selectedEvent: string | null;

  setDate: (date: Date) => void;
  setViewMode: (mode: "month" | "week" | "day") => void;
  selectEvent: (id: string | null) => void;
};

/**
 * Calendar UI state. The calendar is read-only (provider-backed), so there is
 * no event-form or editing state here.
 */
export const useCalendarStore = create<CalendarState>((set) => ({
  selectedDate: new Date(),
  viewMode: "month",
  selectedEvent: null,

  setDate: (date) => set({ selectedDate: date }),
  setViewMode: (mode) => set({ viewMode: mode }),
  selectEvent: (id) => set({ selectedEvent: id }),
}));
