'use client'

import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { cn } from '@/lib/utils'
import { usePreferencesStore } from '../stores/preferences-store'
import {
  DEFAULT_USER_ROLE,
  USER_ROLE_OPTIONS,
  type UserRole,
} from '../types'

interface RoleSelectorProps {
  className?: string
  compact?: boolean
  showLabel?: boolean
}

export function RoleSelector({
  className,
  compact = false,
  showLabel = true,
}: RoleSelectorProps) {
  const role = usePreferencesStore((state) => state.preferences.role ?? DEFAULT_USER_ROLE)
  const setRole = usePreferencesStore((state) => state.setRole)

  return (
    <div className={cn(compact ? 'flex items-center gap-2' : 'space-y-3', className)}>
      {showLabel && (
        <Label className={compact ? 'text-xs font-medium text-muted-foreground' : 'text-sm font-medium'}>
          Rol
        </Label>
      )}
      <Select
        value={role}
        onValueChange={(value) => setRole(value as UserRole)}
      >
        <SelectTrigger
          size={compact ? 'sm' : 'default'}
          className={compact ? 'h-9 w-[82px]' : 'w-[180px]'}
          aria-label="Rol"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {USER_ROLE_OPTIONS.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
