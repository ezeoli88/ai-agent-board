'use client'

import { useMemo, useState } from 'react'
import {
  CheckCircle2,
  Clock,
  Loader2,
  Play,
  RefreshCw,
  TestTube2,
  Terminal,
  XCircle,
} from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { cn } from '@/lib/utils'
import { useTaskQARuns } from '../hooks/use-task-qa-runs'
import type { QaRun, QaRunStatus, Task } from '../types'

interface TaskQAProps {
  task: Task
}

const STATUS_META: Record<QaRunStatus, { label: string; variant: 'secondary' | 'warning' | 'success' | 'destructive' | 'outline'; icon: typeof Clock }> = {
  queued: { label: 'Queued', variant: 'secondary', icon: Clock },
  running: { label: 'Running', variant: 'warning', icon: Loader2 },
  passed: { label: 'Passed', variant: 'success', icon: CheckCircle2 },
  failed: { label: 'Failed', variant: 'destructive', icon: XCircle },
  canceled: { label: 'Canceled', variant: 'outline', icon: XCircle },
}

const DEFAULT_COMMAND = 'npx playwright test --trace on --reporter=line'

export function TaskQA({ task }: TaskQAProps) {
  const [targetUrl, setTargetUrl] = useState('http://localhost:3003')
  const [testCommand, setTestCommand] = useState(DEFAULT_COMMAND)
  const { data: runs = [], isLoading, refetch, startRun } = useTaskQARuns(task.id)
  const hasActiveRun = runs.some((run) => run.status === 'queued' || run.status === 'running')
  const latest = runs[0]

  const canRun = !startRun.isPending && !hasActiveRun && testCommand.trim().length > 0

  const sortedRuns = useMemo(() => runs, [runs])

  return (
    <Card className="overflow-hidden">
      <CardHeader className="py-3">
        <CardTitle className="flex flex-wrap items-center justify-between gap-3 text-lg">
          <span className="flex items-center gap-2">
            <TestTube2 className="h-5 w-5" />
            QA
            {latest ? <QaStatusBadge status={latest.status} /> : null}
          </span>
          <Button variant="ghost" size="icon" onClick={() => refetch()} disabled={isLoading} title="Refresh QA runs">
            <RefreshCw className={cn('h-4 w-4', isLoading && 'animate-spin')} />
          </Button>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-3 md:grid-cols-[1fr_1.4fr_auto]">
          <div className="space-y-1.5">
            <Label htmlFor={`qa-target-${task.id}`}>Target URL</Label>
            <Input
              id={`qa-target-${task.id}`}
              value={targetUrl}
              onChange={(event) => setTargetUrl(event.target.value)}
              placeholder="http://localhost:3003"
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor={`qa-command-${task.id}`}>Command</Label>
            <Input
              id={`qa-command-${task.id}`}
              value={testCommand}
              onChange={(event) => setTestCommand(event.target.value)}
              className="font-mono text-xs"
            />
          </div>
          <div className="flex items-end">
            <Button
              onClick={() =>
                startRun.mutate({
                  target_url: targetUrl.trim() || undefined,
                  test_command: testCommand.trim() || undefined,
                })
              }
              disabled={!canRun}
              className="w-full gap-2 md:w-auto"
            >
              {startRun.isPending || hasActiveRun ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Play className="h-4 w-4" />
              )}
              Run
            </Button>
          </div>
        </div>

        <div className="rounded-md border">
          {sortedRuns.length === 0 ? (
            <div className="flex flex-col items-center gap-2 px-4 py-10 text-center text-muted-foreground">
              <TestTube2 className="h-10 w-10 opacity-50" />
              <p className="text-sm">No QA runs yet.</p>
            </div>
          ) : (
            <div className="divide-y">
              {sortedRuns.map((run) => (
                <QaRunRow key={run.id} run={run} />
              ))}
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  )
}

function QaRunRow({ run }: { run: QaRun }) {
  return (
    <div className="space-y-3 p-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <QaStatusBadge status={run.status} />
            {run.exit_code !== null ? (
              <span className="text-xs text-muted-foreground">exit {run.exit_code}</span>
            ) : null}
            <span className="text-xs text-muted-foreground">{formatDate(run.created_at)}</span>
          </div>
          <p className="break-all font-mono text-xs text-muted-foreground">{run.test_command}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          {run.report_path ? <Badge variant="outline">report</Badge> : null}
          {run.trace_path ? <Badge variant="outline">trace</Badge> : null}
        </div>
      </div>

      {run.target_url ? (
        <p className="break-all text-xs text-muted-foreground">{run.target_url}</p>
      ) : null}

      {(run.stdout || run.stderr) && (
        <details className="rounded-md border bg-muted/20">
          <summary className="flex cursor-pointer items-center gap-2 px-3 py-2 text-sm font-medium">
            <Terminal className="h-4 w-4" />
            Output
          </summary>
          <div className="grid gap-3 border-t p-3">
            {run.stdout ? <QaOutput title="stdout" value={run.stdout} /> : null}
            {run.stderr ? <QaOutput title="stderr" value={run.stderr} muted={false} /> : null}
          </div>
        </details>
      )}
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

function QaOutput({ title, value, muted = true }: { title: string; value: string; muted?: boolean }) {
  return (
    <div className="min-w-0 space-y-1">
      <div className="text-xs font-medium uppercase text-muted-foreground">{title}</div>
      <pre
        className={cn(
          'max-h-72 overflow-auto whitespace-pre-wrap rounded-md border bg-background p-3 font-mono text-xs leading-relaxed',
          muted ? 'text-muted-foreground' : 'text-foreground'
        )}
      >
        {value}
      </pre>
    </div>
  )
}

function formatDate(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return value
  }
  return date.toLocaleString()
}
