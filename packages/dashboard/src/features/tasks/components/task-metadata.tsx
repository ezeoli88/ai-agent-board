'use client'

import { ExternalLink, GitBranch, Calendar, FileCode, Terminal, AlertCircle, CheckCircle, FileText } from 'lucide-react'
import { Link } from '@tanstack/react-router'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { formatDate, extractRepoName } from '@/lib/formatters'
import type { Task } from '../types'

interface TaskMetadataProps {
  task: Task
}

export function TaskMetadata({ task }: TaskMetadataProps) {
  const repoName = extractRepoName(task.repo_url)

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg">Task Details</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid gap-6 sm:grid-cols-2">
          {/* Repository */}
          <div className="space-y-1.5">
            <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <GitBranch className="h-4 w-4" />
              Repository
            </div>
            <a
              href={task.repo_url}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 text-sm hover:underline"
            >
              {repoName}
              <ExternalLink className="h-3.5 w-3.5" />
            </a>
          </div>

          {/* Target Branch */}
          <div className="space-y-1.5">
            <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <GitBranch className="h-4 w-4" />
              Target Branch
            </div>
            <p className="text-sm font-mono">{task.target_branch}</p>
          </div>

          {/* Created Date */}
          <div className="space-y-1.5">
            <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <Calendar className="h-4 w-4" />
              Created
            </div>
            <p className="text-sm">{formatDate(task.created_at)}</p>
          </div>

          {/* Updated Date */}
          <div className="space-y-1.5">
            <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <Calendar className="h-4 w-4" />
              Updated
            </div>
            <p className="text-sm">{formatDate(task.updated_at)}</p>
          </div>

          {/* Context Files */}
          {task.context_files && task.context_files.length > 0 && (
            <div className="space-y-1.5 sm:col-span-2">
              <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                <FileCode className="h-4 w-4" />
                Context Files
              </div>
              <div className="flex flex-wrap gap-2">
                {task.context_files.map((file) => (
                  <Badge key={file} variant="secondary" className="font-mono text-xs">
                    {file}
                  </Badge>
                ))}
              </div>
            </div>
          )}

          {/* Build Command */}
          {task.build_command && (
            <div className="space-y-1.5 sm:col-span-2">
              <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                <Terminal className="h-4 w-4" />
                Build Command
              </div>
              <code className="block rounded-md bg-muted px-3 py-2 text-sm font-mono">
                {task.build_command}
              </code>
            </div>
          )}

          {/* PR URL (if status is done) */}
          {task.status === 'done' && task.pr_url && (
            <div className="space-y-1.5 sm:col-span-2">
              <div className="flex items-center gap-2 text-sm font-medium text-green-600 dark:text-green-400">
                <CheckCircle className="h-4 w-4" />
                Pull Request
              </div>
              <a
                href={task.pr_url}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-sm text-green-600 dark:text-green-400 hover:underline"
              >
                {task.pr_url}
                <ExternalLink className="h-3.5 w-3.5" />
              </a>
            </div>
          )}

          {/* Linked Spec */}
          {task.spec_id && (
            <div className="space-y-1.5 sm:col-span-2">
              <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                <FileText className="h-4 w-4" />
                Spec
              </div>
              <Link
                to="/specs/$specId"
                params={{ specId: task.spec_id }}
                className="inline-flex items-center gap-1.5 text-sm text-primary hover:underline"
              >
                View Spec
                <ExternalLink className="h-3.5 w-3.5" />
              </Link>
            </div>
          )}

          {/* Error message (if status is failed, merge_conflicts, or canceled) */}
          {(task.status === 'failed' || task.status === 'merge_conflicts' || task.status === 'canceled') && task.error && (
            <div className="space-y-1.5 sm:col-span-2">
              <div className="flex items-center gap-2 text-sm font-medium text-destructive">
                <AlertCircle className="h-4 w-4" />
                {task.status === 'merge_conflicts'
                  ? 'Merge Conflicts'
                  : task.status === 'canceled'
                    ? 'Canceled'
                    : 'Error'}
              </div>
              <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {task.error}
              </div>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
