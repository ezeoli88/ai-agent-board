'use client'

import { usePreferencesStore } from '../stores/preferences-store'
import {
  DEFAULT_USER_ROLE,
  canManageSpecs,
  type UserRole,
} from '../types'

export function useCurrentRole(): UserRole {
  return usePreferencesStore((state) => state.preferences.role ?? DEFAULT_USER_ROLE)
}

export function useRolePermissions() {
  const role = useCurrentRole()
  const canManageSpecWorkflow = canManageSpecs(role)

  return {
    role,
    canManageSpecs: canManageSpecWorkflow,
    isSpecReadOnly: !canManageSpecWorkflow,
  }
}
