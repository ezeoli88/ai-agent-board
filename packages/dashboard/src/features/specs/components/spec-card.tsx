'use client'

import { CheckCircle2, GitBranch, Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { formatRelativeTime, truncateText } from '@/lib/formatters'
import { useSpecUIStore } from '../stores/spec-ui-store'
import { SpecStatusBadge } from './spec-status-badge'
import type { Spec } from '../types'

interface SpecCardProps {
  spec: Spec
}

export function SpecCard({ spec }: SpecCardProps) {
  const openDrawer = useSpecUIStore((state) => state.openDrawer)
  const isWorking = false
  const isImplemented = spec.status === 'implemented'

  return (
    <button
      type="button"
      onClick={() => openDrawer(spec.id)}
      className={cn(
        'group w-full rounded-lg border bg-card p-3 text-left shadow-sm',
        'transition-all duration-200 ease-out',
        'hover:border-accent hover:shadow-md hover:-translate-y-0.5',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2'
      )}
    >
      <div className="space-y-2">
        <h4 className="text-sm font-medium leading-tight text-foreground group-hover:text-primary line-clamp-2">
          {truncateText(spec.title, 90)}
        </h4>
        <SpecStatusBadge status={spec.status} className="text-xs" />
      </div>

      <p className="mt-3 line-clamp-3 text-xs leading-relaxed text-muted-foreground">
        {spec.user_input}
      </p>

      <div className="mt-3 flex items-center justify-between text-xs text-muted-foreground">
        <span>{formatRelativeTime(spec.updated_at)}</span>
        <span className="inline-flex items-center gap-1">
          {isWorking ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : isImplemented ? (
            <CheckCircle2 className="h-3 w-3 text-emerald-500" />
          ) : (
            <GitBranch className="h-3 w-3" />
          )}
          {spec.task_ids.length}
        </span>
      </div>
    </button>
  )
}
