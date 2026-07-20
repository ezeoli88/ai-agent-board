/**
 * Theme options
 */
export type Theme = "light" | "dark" | "system";
export type UserRole = "PO" | "TL" | "DEV";

/**
 * Theme option info
 */
export interface ThemeOption {
  value: Theme;
  label: string;
}

export interface UserRoleOption {
  value: UserRole;
  label: string;
}

export interface UserPreferences {
  theme: Theme;
  role: UserRole;
}

/**
 * Available theme options
 */
export const THEME_OPTIONS: ThemeOption[] = [
  { value: "light", label: "Claro" },
  { value: "dark", label: "Oscuro" },
  { value: "system", label: "Sistema" },
];

export const DEFAULT_USER_ROLE: UserRole = "PO";

export const USER_ROLE_OPTIONS: UserRoleOption[] = [
  { value: "PO", label: "PO" },
  { value: "TL", label: "TL" },
  { value: "DEV", label: "DEV" },
];

export function canManageSpecs(role: UserRole): boolean {
  return role === "PO" || role === "TL";
}
