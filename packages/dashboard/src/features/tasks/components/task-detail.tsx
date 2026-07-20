'use client'

import { FileText, ScrollText, GitPullRequest, MessageSquarePlus, MessageSquare, MessageCircle, TestTube2 } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Badge } from '@/components/ui/badge'
import { FEATURE_FLAGS } from '@/config/features'
import { cn } from '@/lib/utils'
import { getAgentDisplayInfo } from '../utils/agent-display'
import type { Task } from '../types'
import { TaskHeader } from './task-header'
import { TaskMetadata } from './task-metadata'
import { TaskDescription } from './task-description'
import { TaskActions } from './task-actions'
import { TaskLogs } from './task-logs'
import { TaskChat } from './task-chat'
import { TaskDiff } from './task-diff'
import { TaskQA } from './task-qa'
import { FeedbackSection } from './feedback-section'
import { PRComments } from './pr-comments'
import { PatternSuggestion } from './pattern-suggestion'
import { MergeConflicts } from './merge-conflicts'
import { AgentModelSelector } from './agent-model-selector'
import { useTaskUIStore } from '../stores/task-ui-store'

interface TaskDetailProps {
  task: Task
}

export function TaskDetail({ task }: TaskDetailProps) {
  const agentInfo = getAgentDisplayInfo(task.agent_type)

  const isDraft = task.status === 'draft'

  // Determine if the task is actively running
  const isActiveTask =
    task.status === 'planning' ||
    task.status === 'in_progress' ||
    task.status === 'coding'

  // Chat tab is enabled for non-draft statuses
  const showChatTab = task.status !== 'draft'

  // Chat is read-only only for terminal statuses
  const isChatReadOnly = ['done', 'failed'].includes(task.status)

  // Determine if changes tab should be enabled
  const showChangesTab =
    task.status === 'awaiting_review' ||
    task.status === 'approved' ||
    task.status === 'pr_created' ||
    task.status === 'changes_requested' ||
    task.status === 'merge_conflicts' ||
    task.status === 'review' ||
    task.status === 'done' ||
    task.status === 'failed'

  // Show PR comments tab when task has a PR
  const showCommentsTab = !!task.pr_url && ['pr_created', 'review', 'changes_requested', 'done'].includes(task.status)

  // Get unread comments count
  const unreadCount = useTaskUIStore((state) => state.getUnreadCount(task.id))

  // Determine default tab based on task status
  const defaultTab = isActiveTask ? 'chat' : 'overview'

  return (
    <div className="space-y-6">
      {/* Header with back button, title, status, and repo link */}
      <TaskHeader task={task} />

      {/* Main content area */}
      <div className="grid gap-6 lg:grid-cols-3">
        {/* Left column - Tabs with Overview, Logs, Changes */}
        <div className="lg:col-span-2">
          <Tabs defaultValue={defaultTab} className="w-full">
            <TabsList variant="line" className="mb-4">
              <TabsTrigger value="overview" className="gap-1.5">
                <FileText className="size-4" />
                Overview
              </TabsTrigger>
              <TabsTrigger value="chat" className="gap-1.5" disabled={!showChatTab}>
                <MessageCircle className="size-4" />
                Chat
                {isActiveTask && (
                  <span className="ml-1 size-2 rounded-full bg-green-500 animate-pulse" />
                )}
              </TabsTrigger>
              <TabsTrigger value="logs" className="gap-1.5">
                <ScrollText className="size-4" />
                Logs
              </TabsTrigger>
              <TabsTrigger value="changes" className="gap-1.5" disabled={!showChangesTab}>
                <GitPullRequest className="size-4" />
                Changes
              </TabsTrigger>
              {FEATURE_FLAGS.qaDashboard && (
                <TabsTrigger value="qa" className="gap-1.5">
                  <TestTube2 className="size-4" />
                  QA
                </TabsTrigger>
              )}
              <TabsTrigger value="comments" className="gap-1.5" disabled={!showCommentsTab}>
                <MessageSquare className="size-4" />
                Comments
                {unreadCount > 0 && (
                  <Badge variant="default" className="ml-1 h-5 min-w-5 bg-blue-500 px-1.5 text-xs">
                    {unreadCount}
                  </Badge>
                )}
              </TabsTrigger>
            </TabsList>

            <TabsContent value="overview" className="space-y-6 mt-0">
              {task.status === 'merge_conflicts' && (
                <MergeConflicts task={task} />
              )}

              <TaskDescription task={task} showRebuildSpec />

              {/* Agent / Model selector */}
              <AgentModelSelector task={task} />

              {/* Show pattern suggestion when changes were requested */}
              {task.status === 'changes_requested' && task.repository_id && (
                <PatternSuggestion task={task} />
              )}

              <TaskMetadata task={task} />
            </TabsContent>

            <TabsContent value="chat" className="mt-0">
              <Card className="overflow-hidden">
                <CardContent className="p-0">
                  <div className="h-[500px]">
                    <TaskChat
                      task={task}
                      readOnly={isChatReadOnly}
                    />
                  </div>
                </CardContent>
              </Card>
            </TabsContent>

            <TabsContent value="logs" className="mt-0">
              <Card className="overflow-hidden">
                <CardHeader className="py-3">
                  <CardTitle className="flex items-center gap-2 text-lg">
                    <ScrollText className="h-5 w-5" />
                    Execution Logs
                    {task.agent_type && (
                      <span className={cn(
                        'inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-normal',
                        agentInfo?.colorClasses.bg ?? 'bg-muted',
                        agentInfo?.colorClasses.text ?? 'text-muted-foreground'
                      )}>
                        {agentInfo?.icon} {agentInfo?.name ?? task.agent_type}
                      </span>
                    )}
                  </CardTitle>
                </CardHeader>
                <CardContent className="p-0">
                  <div className="h-[500px]">
                    <TaskLogs
                      task={task}
                      enabled={true}
                      showFeedbackForm={false}
                    />
                  </div>
                </CardContent>
              </Card>
            </TabsContent>

            <TabsContent value="changes" className="mt-0">
              {showChangesTab ? (
                <TaskDiff taskId={task.id} />
              ) : (
                <Card>
                  <CardHeader>
                    <CardTitle className="flex items-center gap-2 text-lg">
                      <GitPullRequest className="h-5 w-5" />
                      Code Changes
                    </CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="text-center py-8 text-muted-foreground">
                      <GitPullRequest className="h-12 w-12 mx-auto mb-4 opacity-50" />
                      <p className="text-sm">No changes available yet.</p>
                      <p className="text-xs mt-1">Execute the task to generate changes.</p>
                    </div>
                  </CardContent>
                </Card>
              )}
            </TabsContent>

            {FEATURE_FLAGS.qaDashboard && (
              <TabsContent value="qa" className="mt-0">
                <TaskQA task={task} />
              </TabsContent>
            )}

            <TabsContent value="comments" className="mt-0">
              {showCommentsTab ? (
                <PRComments task={task} />
              ) : (
                <Card>
                  <CardHeader>
                    <CardTitle className="flex items-center gap-2 text-lg">
                      <MessageSquare className="h-5 w-5" />
                      PR Comments
                    </CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="text-center py-8 text-muted-foreground">
                      <MessageSquare className="h-12 w-12 mx-auto mb-4 opacity-50" />
                      <p className="text-sm">No PR created yet.</p>
                      <p className="text-xs mt-1">Comments will appear here once a PR is created.</p>
                    </div>
                  </CardContent>
                </Card>
              )}
            </TabsContent>
          </Tabs>
        </div>

        {/* Right column - Actions */}
        <div>
          <TaskActions task={task} />
        </div>
      </div>
    </div>
  )
}
