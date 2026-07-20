// Components
export {
  ChromeMcpSection,
  ConnectionsSection,
  PreferencesSection,
  RoleSelector,
  ThemeSelector,
  SettingsTour,
} from './components'

// Stores
export { usePreferencesStore } from './stores/preferences-store'

// Hooks
export { useCurrentRole, useRolePermissions } from './hooks/use-role-permissions'

// Types
export type {
  Theme,
  UserRole,
  UserPreferences,
} from './types'

export {
  DEFAULT_USER_ROLE,
  THEME_OPTIONS,
  USER_ROLE_OPTIONS,
  canManageSpecs,
} from './types'
