'use client'

import { useCallback, useState, useEffect } from 'react'
import { Search, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { useTaskUIStore } from '../stores/task-ui-store'
import { TASK_STATUSES, TASK_STATUS_LABELS, type TaskStatus } from '../types'

interface TaskFiltersProps {
  className?: string
  compact?: boolean
}

export function TaskFilters({ className, compact = false }: TaskFiltersProps) {
  const { statusFilter, searchQuery, setStatusFilter, setSearchQuery, clearFilters } =
    useTaskUIStore()

  // Local state for debounced search
  const [localSearch, setLocalSearch] = useState(searchQuery)

  // Debounce search input
  useEffect(() => {
    const timer = setTimeout(() => {
      setSearchQuery(localSearch)
    }, 300)

    return () => clearTimeout(timer)
  }, [localSearch, setSearchQuery])

  // Sync local search with store when store changes externally
  useEffect(() => {
    setLocalSearch(searchQuery)
  }, [searchQuery])

  const handleStatusToggle = useCallback(
    (status: TaskStatus) => {
      if (statusFilter.includes(status)) {
        setStatusFilter(statusFilter.filter((s) => s !== status))
      } else {
        setStatusFilter([...statusFilter, status])
      }
    },
    [statusFilter, setStatusFilter]
  )

  const hasActiveFilters = statusFilter.length > 0 || searchQuery.length > 0

  if (compact) {
    return (
      <div className={cn('space-y-3', className)}>
        {/* Search input */}
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            type="search"
            placeholder="Search tasks..."
            value={localSearch}
            onChange={(e) => setLocalSearch(e.target.value)}
            className="pl-8 h-8 text-sm"
          />
        </div>

        {/* Status filter chips - compact scrollable */}
        <div className="flex flex-wrap gap-1">
          {TASK_STATUSES.map((status) => (
            <StatusChip
              key={status}
              status={status}
              isActive={statusFilter.includes(status)}
              onClick={() => handleStatusToggle(status)}
              compact
            />
          ))}
        </div>

        {/* Clear filters */}
        {hasActiveFilters && (
          <Button
            variant="ghost"
            size="sm"
            onClick={clearFilters}
            className="h-7 px-2 text-xs"
          >
            <X className="mr-1 h-3 w-3" />
            Clear filters
          </Button>
        )}
      </div>
    )
  }

  return (
    <div className={cn('space-y-4', className)}>
      {/* Search input */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          type="search"
          placeholder="Search tasks..."
          value={localSearch}
          onChange={(e) => setLocalSearch(e.target.value)}
          className="pl-9"
        />
      </div>

      {/* Status filter chips */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium text-foreground">Status</span>
          {hasActiveFilters && (
            <Button
              variant="ghost"
              size="sm"
              onClick={clearFilters}
              className="h-7 px-2 text-xs"
            >
              <X className="mr-1 h-3 w-3" />
              Clear all
            </Button>
          )}
        </div>
        <div className="flex flex-wrap gap-2">
          {TASK_STATUSES.map((status) => (
            <StatusChip
              key={status}
              status={status}
              isActive={statusFilter.includes(status)}
              onClick={() => handleStatusToggle(status)}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

interface StatusChipProps {
  status: TaskStatus
  isActive: boolean
  onClick: () => void
  compact?: boolean
}

const STATUS_COLORS: Record<TaskStatus, { active: string; inactive: string }> = {
  // New two-agent workflow statuses
  draft: {
    active: 'bg-gray-500 text-white hover:bg-gray-600',
    inactive: 'bg-gray-100 text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700',
  },
  refining: {
    active: 'bg-blue-500 text-white hover:bg-blue-600',
    inactive: 'bg-blue-100 text-blue-700 hover:bg-blue-200 dark:bg-blue-900 dark:text-blue-300 dark:hover:bg-blue-800',
  },
  pending_approval: {
    active: 'bg-amber-500 text-white hover:bg-amber-600',
    inactive: 'bg-amber-100 text-amber-700 hover:bg-amber-200 dark:bg-amber-900 dark:text-amber-300 dark:hover:bg-amber-800',
  },
  approved: {
    active: 'bg-green-500 text-white hover:bg-green-600',
    inactive: 'bg-green-100 text-green-700 hover:bg-green-200 dark:bg-green-900 dark:text-green-300 dark:hover:bg-green-800',
  },
  coding: {
    active: 'bg-yellow-500 text-white hover:bg-yellow-600',
    inactive: 'bg-yellow-100 text-yellow-700 hover:bg-yellow-200 dark:bg-yellow-900 dark:text-yellow-300 dark:hover:bg-yellow-800',
  },
  plan_review: {
    active: 'bg-orange-500 text-white hover:bg-orange-600',
    inactive: 'bg-orange-100 text-orange-700 hover:bg-orange-200 dark:bg-orange-900 dark:text-orange-300 dark:hover:bg-orange-800',
  },
  review: {
    active: 'bg-purple-500 text-white hover:bg-purple-600',
    inactive: 'bg-purple-100 text-purple-700 hover:bg-purple-200 dark:bg-purple-900 dark:text-purple-300 dark:hover:bg-purple-800',
  },
  changes_requested: {
    active: 'bg-orange-500 text-white hover:bg-orange-600',
    inactive: 'bg-orange-100 text-orange-700 hover:bg-orange-200 dark:bg-orange-900 dark:text-orange-300 dark:hover:bg-orange-800',
  },
  done: {
    active: 'bg-emerald-500 text-white hover:bg-emerald-600',
    inactive: 'bg-emerald-100 text-emerald-700 hover:bg-emerald-200 dark:bg-emerald-900 dark:text-emerald-300 dark:hover:bg-emerald-800',
  },
  failed: {
    active: 'bg-red-500 text-white hover:bg-red-600',
    inactive: 'bg-red-100 text-red-700 hover:bg-red-200 dark:bg-red-900 dark:text-red-300 dark:hover:bg-red-800',
  },
  // Legacy statuses (for backward compatibility)
  backlog: {
    active: 'bg-gray-500 text-white hover:bg-gray-600',
    inactive: 'bg-gray-100 text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700',
  },
  planning: {
    active: 'bg-blue-500 text-white hover:bg-blue-600',
    inactive: 'bg-blue-100 text-blue-700 hover:bg-blue-200 dark:bg-blue-900 dark:text-blue-300 dark:hover:bg-blue-800',
  },
  in_progress: {
    active: 'bg-yellow-500 text-white hover:bg-yellow-600',
    inactive: 'bg-yellow-100 text-yellow-700 hover:bg-yellow-200 dark:bg-yellow-900 dark:text-yellow-300 dark:hover:bg-yellow-800',
  },
  awaiting_review: {
    active: 'bg-purple-500 text-white hover:bg-purple-600',
    inactive: 'bg-purple-100 text-purple-700 hover:bg-purple-200 dark:bg-purple-900 dark:text-purple-300 dark:hover:bg-purple-800',
  },
  pr_created: {
    active: 'bg-indigo-500 text-white hover:bg-indigo-600',
    inactive: 'bg-indigo-100 text-indigo-700 hover:bg-indigo-200 dark:bg-indigo-900 dark:text-indigo-300 dark:hover:bg-indigo-800',
  },
  canceled: {
    active: 'bg-gray-500 text-white hover:bg-gray-600',
    inactive: 'bg-gray-100 text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700',
  },
  merge_conflicts: {
    active: 'bg-orange-500 text-white hover:bg-orange-600',
    inactive: 'bg-orange-100 text-orange-700 hover:bg-orange-200 dark:bg-orange-900 dark:text-orange-300 dark:hover:bg-orange-800',
  },
}

function StatusChip({ status, isActive, onClick, compact }: StatusChipProps) {
  const colors = STATUS_COLORS[status]
  const label = TASK_STATUS_LABELS[status]

  return (
    <Badge
      className={cn(
        'cursor-pointer select-none transition-colors',
        isActive ? colors.active : colors.inactive,
        compact && 'text-xs px-1.5 py-0'
      )}
      onClick={onClick}
    >
      {label}
    </Badge>
  )
}
