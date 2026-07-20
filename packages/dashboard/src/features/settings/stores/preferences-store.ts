"use client";

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { DEFAULT_USER_ROLE, type UserRole } from "../types";

/**
 * Theme options
 */
export type Theme = "light" | "dark" | "system";

/**
 * User preferences interface
 */
export interface UserPreferences {
  theme: Theme;
  role: UserRole;
}

/**
 * Default preferences
 */
const DEFAULT_PREFERENCES: UserPreferences = {
  theme: "system",
  role: DEFAULT_USER_ROLE,
};

/**
 * Preferences store state interface
 */
interface PreferencesState {
  // Preferences data
  preferences: UserPreferences;

  // Actions
  setTheme: (theme: Theme) => void;
  setRole: (role: UserRole) => void;

  // Reset
  resetPreferences: () => void;
}

export const usePreferencesStore = create<PreferencesState>()(
  persist(
    (set) => ({
      // Initial state
      preferences: DEFAULT_PREFERENCES,

      // Actions
      setTheme: (theme) =>
        set((state) => ({
          preferences: {
            ...state.preferences,
            theme,
          },
        })),
      setRole: (role) =>
        set((state) => ({
          preferences: {
            ...state.preferences,
            role,
          },
        })),

      // Reset
      resetPreferences: () =>
        set({
          preferences: DEFAULT_PREFERENCES,
        }),
    }),
    {
      name: "dash-agent-preferences",
      merge: (persisted, current) => {
        const persistedState = persisted as Partial<PreferencesState> | undefined;
        return {
          ...current,
          ...persistedState,
          preferences: {
            ...current.preferences,
            ...(persistedState?.preferences ?? {}),
          },
        };
      },
    },
  ),
);
