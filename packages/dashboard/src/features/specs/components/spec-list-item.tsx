'use client'

import { cn } from '@/lib/utils'
import { formatRelativeTime, truncateText } from '@/lib/formatters'
import { SpecStatusBadge } from './spec-status-badge'
import { useSpecUIStore } from '../stores/spec-ui-store'
import type { Spec } from '../types'

interface SpecListItemProps {
  spec: Spec
  isSelected?: boolean
}

export function SpecListItem({ spec, isSelected }: SpecListItemProps) {
  const isRefining = spec.status === 'refining'
  const openDrawer = useSpecUIStore((state) => state.openDrawer)

  return (
    <button
      type="button"
      onClick={() => openDrawer(spec.id)}
      className={cn(
        'group flex items-center gap-3 rounded-lg border border-border bg-card p-3 w-full text-left',
        'transition-all duration-200 ease-out',
        'hover:border-accent hover:bg-accent/50 hover:shadow-sm hover:-translate-y-0.5',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
        isSelected && 'border-primary bg-primary/5',
        isRefining && 'animate-pulse-subtle'
      )}
    >
      {/* Status indicator dot */}
      <div
        className={cn(
          'h-2 w-2 shrink-0 rounded-full',
          spec.status === 'draft' && 'bg-gray-400',
          spec.status === 'refining' && 'bg-blue-500',
          spec.status === 'approved' && 'bg-green-500',
          spec.status === 'failed' && 'bg-red-500',
          spec.status === 'canceled' && 'bg-gray-400'
        )}
      />

      {/* Content */}
      <div className="min-w-0 flex-1">
        <h4 className="truncate text-sm font-medium text-foreground group-hover:text-primary">
          {truncateText(spec.title, 60)}
        </h4>
        <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
          <span className="truncate max-w-[200px]">
            {truncateText(spec.user_input, 50)}
          </span>
          <span className="shrink-0">&#183;</span>
          <span className="shrink-0">{formatRelativeTime(spec.updated_at)}</span>
        </div>
      </div>

      {/* Status badge */}
      <SpecStatusBadge status={spec.status} className="shrink-0" />
    </button>
  )
}
