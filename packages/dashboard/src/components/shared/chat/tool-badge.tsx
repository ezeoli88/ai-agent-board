import {
  FileText,
  Pencil,
  Terminal,
  FilePlus,
  Search,
  Loader2,
  Check,
  X,
} from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import type { ToolActivityEvent } from '@dash-agent/shared'

function getToolIcon(name: string) {
  const lower = name.toLowerCase()
  if (lower === 'read') return FileText
  if (lower === 'edit') return Pencil
  if (lower === 'bash') return Terminal
  if (lower === 'write') return FilePlus
  if (lower === 'grep' || lower === 'glob') return Search
  return Terminal
}

export function ToolBadge({ activity }: { activity: ToolActivityEvent }) {
  const Icon = getToolIcon(activity.name)
  const displayName = activity.name || 'Tool'

  // Clean up summary: remove long absolute paths, keep just the meaningful part
  const cleanSummary = activity.summary
    ? activity.summary
        .replace(/^.*[/\\]worktrees[/\\][^/\\]+[/\\]/, '') // strip worktree prefix
        .replace(/^.*[/\\](?=[^/\\]+$)/, '') // for single file, keep just filename
    : ''

  return (
    <div className="flex items-center py-0.5">
      <Badge
        variant="secondary"
        className="gap-1.5 px-2 py-0.5 text-xs font-normal bg-zinc-800 dark:bg-zinc-200 text-zinc-300 dark:text-zinc-600 border-0 max-w-full"
      >
        {activity.status === 'running' ? (
          <Loader2 className="size-3 animate-spin text-blue-400 dark:text-blue-600 shrink-0" />
        ) : activity.status === 'completed' ? (
          <Check className="size-3 text-emerald-400 dark:text-emerald-600 shrink-0" />
        ) : activity.status === 'error' ? (
          <X className="size-3 text-red-400 dark:text-red-600 shrink-0" />
        ) : (
          <Icon className="size-3 shrink-0" />
        )}
        <span className="font-medium text-zinc-200 dark:text-zinc-700">{displayName}</span>
        {cleanSummary && (
          <span className="truncate max-w-[300px] text-zinc-400 dark:text-zinc-500">{cleanSummary}</span>
        )}
      </Badge>
    </div>
  )
}
