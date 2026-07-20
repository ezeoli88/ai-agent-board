'use client'

import { useMemo, useState } from 'react'
import { Link } from '@tanstack/react-router'
import {
  AlertCircle,
  ArrowLeftRight,
  CheckCircle2,
  Clock,
  FolderGit2,
  Loader2,
  Play,
  RefreshCw,
  Terminal,
  TestTube2,
  XCircle,
} from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ScrollArea } from '@/components/ui/scroll-area'
import { StatusBadge } from '@/components/shared/status-badge'
import { formatRelativeTime, truncateText } from '@/lib/formatters'
import { cn } from '@/lib/utils'
import { useRepos, useRepoStore } from '@/features/repos'
import { taskKeys } from '@/features/tasks/hooks/query-keys'
import { useQABoard } from '@/features/tasks/hooks/use-qa-board'
import { useTaskUIStore } from '@/features/tasks/stores/task-ui-store'
import type { QaBoardItem, QaRunStatus } from '@/features/tasks/types'

type QaColumnId = 'ready' | 'running' | 'failed' | 'passed'

interface QaColumnConfig {
  id: QaColumnId
  title: string
  color: string
  bgColor: string
  borderColor: string
  icon: typeof TestTube2
}

const DEFAULT_COMMAND = 'npx playwright test --trace on --reporter=line'

const QA_COLUMNS: QaColumnConfig[] = [
  {
    id: 'ready',
    title: 'Ready',
    color: 'text-slate-700 dark:text-slate-300',
    bgColor: 'bg-slate-50 dark:bg-slate-900/20',
    borderColor: 'border-slate-300 dark:border-slate-700',
    icon: Clock,
  },
  {
    id: 'running',
    title: 'Running',
    color: 'text-blue-700 dark:text-blue-300',
    bgColor: 'bg-blue-50 dark:bg-blue-950/20',
    borderColor: 'border-blue-300 dark:border-blue-700',
    icon: Loader2,
  },
  {
    id: 'failed',
    title: 'Failed',
    color: 'text-red-700 dark:text-red-300',
    bgColor: 'bg-red-50 dark:bg-red-950/20',
    borderColor: 'border-red-300 dark:border-red-700',
    icon: XCircle,
  },
  {
    id: 'passed',
    title: 'Passed',
    color: 'text-green-700 dark:text-green-300',
    bgColor: 'bg-green-50 dark:bg-green-950/20',
    borderColor: 'border-green-300 dark:border-green-700',
    icon: CheckCircle2,
  },
]

const STATUS_META: Record<QaRunStatus, { label: string; variant: 'secondary' | 'warning' | 'success' | 'destructive' | 'outline'; icon: typeof Clock }> = {
  queued: { label: 'Queued', variant: 'secondary', icon: Clock },
  running: { label: 'Running', variant: 'warning', icon: Loader2 },
  passed: { label: 'Passed', variant: 'success', icon: CheckCircle2 },
  failed: { label: 'Failed', variant: 'destructive', icon: XCircle },
  canceled: { label: 'Canceled', variant: 'outline', icon: XCircle },
}

export function QABoardView() {
  const queryClient = useQueryClient()
  const { data: repos } = useRepos()
  const selectedRepo = useRepoStore((s) => s.selectedRepo)
  const selectedRepoId = useRepoStore((s) => s.selectedRepoId)
  const [targetUrl, setTargetUrl] = useState('http://localhost:3003')
  const [testCommand, setTestCommand] = useState(DEFAULT_COMMAND)
  const { data: items = [], isLoading, isError, error, startRun } = useQABoard({
    repositoryId: selectedRepoId ?? undefined,
  })

  const repo = useMemo(() => {
    if (selectedRepo) return selectedRepo
    if (selectedRepoId && repos) return repos.find((r) => r.id === selectedRepoId) ?? repos[0] ?? null
    return repos?.[0] ?? null
  }, [selectedRepo, selectedRepoId, repos])

  const columns = useMemo(() => {
    const grouped: Record<QaColumnId, QaBoardItem[]> = {
      ready: [],
      running: [],
      failed: [],
      passed: [],
    }

    items.forEach((item) => {
      grouped[getColumnForItem(item)].push(item)
    })

    Object.values(grouped).forEach((columnItems) => {
      columnItems.sort((a, b) => getItemTimestamp(b) - getItemTimestamp(a))
    })

    return grouped
  }, [items])

  const handleRefresh = () => {
    queryClient.invalidateQueries({ queryKey: taskKeys.qaBoards() })
  }

  const runQa = (taskId: string) => {
    startRun.mutate({
      taskId,
      input: {
        target_url: targetUrl.trim() || undefined,
        test_command: testCommand.trim() || undefined,
      },
    })
  }

  if (isError) {
    return (
      <div className="flex flex-col gap-6">
        <QAHeader repoName={repo?.name} onRefresh={handleRefresh} isRefreshing={false} />
        <div className="flex flex-col items-center justify-center gap-4 rounded-lg border border-destructive/50 bg-destructive/10 p-8">
          <AlertCircle className="h-8 w-8 text-destructive" />
          <div className="text-center">
            <h3 className="font-semibold text-destructive">Failed to load QA</h3>
            <p className="text-sm text-muted-foreground">
              {error instanceof Error ? error.message : 'An unexpected error occurred'}
            </p>
          </div>
          <Button variant="outline" onClick={handleRefresh} className="gap-2">
            <RefreshCw className="h-4 w-4" />
            Retry
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      <QAHeader repoName={repo?.name} onRefresh={handleRefresh} isRefreshing={isLoading} />

      <div className="grid gap-3 rounded-lg border bg-card p-3 md:grid-cols-[1fr_1.5fr_auto]">
        <div className="space-y-1.5">
          <Label htmlFor="qa-board-target">Target URL</Label>
          <Input
            id="qa-board-target"
            value={targetUrl}
            onChange={(event) => setTargetUrl(event.target.value)}
            placeholder="http://localhost:3003"
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="qa-board-command">Command</Label>
          <Input
            id="qa-board-command"
            value={testCommand}
            onChange={(event) => setTestCommand(event.target.value)}
            className="font-mono text-xs"
          />
        </div>
        <div className="flex items-end">
          <Button variant="outline" onClick={handleRefresh} className="w-full gap-2 md:w-auto">
            <RefreshCw className={cn('h-4 w-4', isLoading && 'animate-spin')} />
            Refresh
          </Button>
        </div>
      </div>

      <div className="-mx-4 flex gap-4 overflow-x-auto px-4 pb-4 md:-mx-6 md:px-6">
        {QA_COLUMNS.map((column) => (
          <QAColumn
            key={column.id}
            config={column}
            items={columns[column.id]}
            isLoading={isLoading}
            pendingTaskId={startRun.variables?.taskId}
            isStarting={startRun.isPending}
            onRun={runQa}
          />
        ))}
      </div>
    </div>
  )
}

function QAHeader({
  repoName,
  onRefresh,
  isRefreshing,
}: {
  repoName?: string
  onRefresh: () => void
  isRefreshing: boolean
}) {
  return (
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div className="flex items-center gap-3">
        <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10">
          <TestTube2 className="h-5 w-5 text-primary" />
        </div>
        <div>
          <h1 className="text-xl font-semibold tracking-tight">QA</h1>
          {repoName ? (
            <p className="flex items-center gap-1.5 text-sm text-muted-foreground">
              <FolderGit2 className="h-3.5 w-3.5" />
              {repoName}
            </p>
          ) : null}
        </div>
      </div>

      <div className="flex items-center gap-2">
        <Button variant="ghost" size="icon" onClick={onRefresh} disabled={isRefreshing} title="Refresh QA">
          <RefreshCw className={cn('h-4 w-4', isRefreshing && 'animate-spin')} />
        </Button>
        <Button variant="ghost" size="sm" asChild>
          <Link to="/repos">
            <ArrowLeftRight className="h-4 w-4" />
            Cambiar repo
          </Link>
        </Button>
      </div>
    </header>
  )
}

function QAColumn({
  config,
  items,
  isLoading,
  pendingTaskId,
  isStarting,
  onRun,
}: {
  config: QaColumnConfig
  items: QaBoardItem[]
  isLoading: boolean
  pendingTaskId?: string
  isStarting: boolean
  onRun: (taskId: string) => void
}) {
  const Icon = config.icon
  const hasRunningTask = items.some((item) => item.latest_run?.status === 'running')

  return (
    <div
      className={cn(
        'flex h-full w-[320px] shrink-0 flex-col overflow-hidden rounded-lg border',
        config.borderColor,
        config.bgColor
      )}
    >
      <div className="flex items-center justify-between border-b border-inherit px-3 py-2.5">
        <div className="flex items-center gap-2">
          <Icon
            className={cn(
              'h-4 w-4',
              config.id === 'running' && hasRunningTask && 'animate-spin',
              config.color
            )}
          />
          <h3 className={cn('text-sm font-semibold', config.color)}>{config.title}</h3>
          <span className={cn('inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-background/80 px-1.5 text-xs font-medium', config.color)}>
            {items.length}
          </span>
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="space-y-2 p-2 pr-4">
          {isLoading ? (
            <>
              <QACardSkeleton />
              <QACardSkeleton />
            </>
          ) : items.length === 0 ? (
            <div className="flex h-20 items-center justify-center rounded-md border border-dashed border-muted-foreground/25">
              <p className="text-xs text-muted-foreground">No tasks</p>
            </div>
          ) : (
            items.map((item) => (
              <QACard
                key={item.task.id}
                item={item}
                isStarting={isStarting && pendingTaskId === item.task.id}
                onRun={() => onRun(item.task.id)}
              />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  )
}

function QACard({
  item,
  isStarting,
  onRun,
}: {
  item: QaBoardItem
  isStarting: boolean
  onRun: () => void
}) {
  const openDrawer = useTaskUIStore((state) => state.openDrawer)
  const run = item.latest_run
  const isActive = run?.status === 'queued' || run?.status === 'running'

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => openDrawer(item.task.id)}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault()
          openDrawer(item.task.id)
        }
      }}
      className="group w-full min-w-0 overflow-hidden rounded-lg border bg-card p-3 shadow-sm transition-all duration-200 hover:border-accent hover:shadow-md focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
    >
      <div className="space-y-2">
        <div className="min-w-0 space-y-1.5">
          <h4 className="line-clamp-3 min-w-0 break-words text-sm font-medium leading-tight text-foreground group-hover:text-primary">
            {truncateText(item.task.title, 90)}
          </h4>
          <StatusBadge status={item.task.status} className="max-w-full text-xs" />
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {run ? (
            <Link
              to="/qa/runs/$runId"
              params={{ runId: run.id }}
              onClick={(event) => event.stopPropagation()}
            >
              <QaStatusBadge status={run.status} />
            </Link>
          ) : (
            <Badge variant="outline">No QA</Badge>
          )}
          {run?.exit_code !== null && run?.exit_code !== undefined ? (
            <span className="text-xs text-muted-foreground">exit {run.exit_code}</span>
          ) : null}
        </div>

        {run ? (
          <div className="space-y-1 text-xs text-muted-foreground">
            <p className="break-all font-mono">{truncateText(run.test_command, 110)}</p>
            {run.target_url ? <p className="break-all">{run.target_url}</p> : null}
            <p>{formatRelativeTime(run.updated_at)}</p>
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">{formatRelativeTime(item.task.updated_at)}</p>
        )}

        {run ? (
          <Button
            variant="outline"
            size="sm"
            className="w-full min-w-0 gap-2 px-3"
            asChild
          >
            <Link
              to="/qa/runs/$runId"
              params={{ runId: run.id }}
              onClick={(event) => event.stopPropagation()}
            >
              <Terminal className="h-4 w-4" />
              View logs
            </Link>
          </Button>
        ) : null}

        <Button
          size="sm"
          className="w-full min-w-0 gap-2 px-3"
          disabled={isActive || isStarting}
          onClick={(event) => {
            event.stopPropagation()
            onRun()
          }}
        >
          {isStarting || isActive ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Play className="h-4 w-4" />
          )}
          Run QA
        </Button>
      </div>
    </div>
  )
}

function QaStatusBadge({ status }: { status: QaRunStatus }) {
  const meta = STATUS_META[status]
  const Icon = meta.icon
  return (
    <Badge variant={meta.variant} className="gap-1">
      <Icon className={cn('h-3 w-3', status === 'running' && 'animate-spin')} />
      {meta.label}
    </Badge>
  )
}

function QACardSkeleton() {
  return (
    <div className="space-y-3 rounded-lg border bg-card p-3 shadow-sm">
      <div className="h-4 w-4/5 rounded bg-muted" />
      <div className="flex gap-2">
        <div className="h-5 w-16 rounded-full bg-muted" />
        <div className="h-5 w-20 rounded-full bg-muted" />
      </div>
      <div className="h-8 rounded bg-muted" />
      <div className="h-8 rounded-full bg-muted" />
    </div>
  )
}

function getColumnForItem(item: QaBoardItem): QaColumnId {
  switch (item.latest_run?.status) {
    case 'queued':
    case 'running':
      return 'running'
    case 'failed':
      return 'failed'
    case 'passed':
      return 'passed'
    case 'canceled':
    default:
      return 'ready'
  }
}

function getItemTimestamp(item: QaBoardItem) {
  const value = item.latest_run?.updated_at ?? item.task.updated_at
  const timestamp = new Date(value).getTime()
  return Number.isNaN(timestamp) ? 0 : timestamp
}
