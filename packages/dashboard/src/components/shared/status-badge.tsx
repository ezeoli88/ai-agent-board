import { Badge } from '@/components/ui/badge'
import type { TaskStatus } from '@/features/tasks/types'

const STATUS_CONFIG: Record<
  TaskStatus,
  {
    label: string
    variant:
      | 'default'
      | 'secondary'
      | 'destructive'
      | 'outline'
      | 'warning'
      | 'success'
      | 'purple'
      | 'indigo'
      | 'orange'
  }
> = {
  // New two-agent workflow statuses
  draft: { label: 'Draft', variant: 'secondary' },
  refining: { label: 'Generating Spec', variant: 'default' },
  pending_approval: { label: 'Review Spec', variant: 'orange' },
  approved: { label: 'Approved', variant: 'success' },
  coding: { label: 'Coding', variant: 'warning' },
  plan_review: { label: 'Review Plan', variant: 'orange' },
  review: { label: 'PR Review', variant: 'purple' },
  changes_requested: { label: 'Changes Requested', variant: 'orange' },
  done: { label: 'Done', variant: 'success' },
  failed: { label: 'Failed', variant: 'destructive' },
  // Legacy statuses (for backward compatibility)
  backlog: { label: 'Backlog', variant: 'secondary' },
  planning: { label: 'Planning', variant: 'default' },
  in_progress: { label: 'In Progress', variant: 'warning' },
  awaiting_review: { label: 'Awaiting Review', variant: 'purple' },
  pr_created: { label: 'PR Created', variant: 'indigo' },
  canceled: { label: 'Canceled', variant: 'secondary' },
  merge_conflicts: { label: 'Merge Conflicts', variant: 'orange' },
}

interface StatusBadgeProps {
  status: TaskStatus
  className?: string
}

export function StatusBadge({ status, className }: StatusBadgeProps) {
  // Defensive check for invalid status values
  const config = STATUS_CONFIG[status]
  if (!config) {
    console.error(`StatusBadge: Unknown status "${status}"`)
    return (
      <Badge variant="secondary" className={className}>
        {status || 'Unknown'}
      </Badge>
    )
  }
  return (
    <Badge variant={config.variant} className={className}>
      {config.label}
    </Badge>
  )
}
