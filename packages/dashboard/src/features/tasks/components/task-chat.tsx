'use client'

import { useEffect, useRef, useState, useCallback } from 'react'
import {
  MessageSquare,
  Loader2,
  Send,
  Keyboard,
  AlertCircle,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { useQueryClient } from '@tanstack/react-query'
import { cn } from '@/lib/utils'
import { ChatMessageBubble, ToolBadge } from '@/components/shared/chat'
import { taskKeys } from '../hooks/query-keys'
import { useTaskChat } from '../hooks/use-task-chat'
import type { ChatEntry } from '../hooks/use-task-chat'
import { useTaskActions } from '../hooks/use-task-actions'
import { toast } from 'sonner'
import type { Task } from '../types'
import type { ChatMessageEvent, ToolActivityEvent } from '@dash-agent/shared'

interface TaskChatProps {
  task: Task
  readOnly?: boolean
  className?: string
}

// ============================================================================
// ChatInput
// ============================================================================

function ChatInput({ taskId, disabled, disabledReason, onMessageSent, placeholder }: { taskId: string; disabled: boolean; disabledReason?: string; onMessageSent?: (content: string) => void; placeholder?: string }) {
  const [message, setMessage] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const { sendFeedback } = useTaskActions(taskId)

  const canSubmit = message.trim().length > 0 && !sendFeedback.isPending && !disabled

  const handleSubmit = useCallback(() => {
    if (!canSubmit) return
    const trimmed = message.trim()
    // Show the message in chat immediately (optimistic)
    onMessageSent?.(trimmed)
    setMessage('')
    sendFeedback.mutate(trimmed, {
      onSuccess: () => {
        textareaRef.current?.focus()
      },
    })
  }, [canSubmit, message, sendFeedback, onMessageSent])

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSubmit()
    }
  }, [handleSubmit])

  if (disabled) {
    if (disabledReason) {
      return (
        <div className="border-t bg-background px-4 py-3">
          <p className="text-sm text-muted-foreground text-center">{disabledReason}</p>
        </div>
      )
    }
    return null
  }

  return (
    <div className="border-t bg-background p-3 space-y-2">
      <div className="flex gap-2">
        <Textarea
          ref={textareaRef}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder || "Send a message to the agent..."}
          disabled={sendFeedback.isPending}
          className="min-h-[40px] max-h-[120px] resize-none text-sm"
          rows={1}
        />
        <Button
          size="icon"
          onClick={handleSubmit}
          disabled={!canSubmit}
          className="shrink-0 self-end"
        >
          {sendFeedback.isPending ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <Send className="size-4" />
          )}
        </Button>
      </div>
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Keyboard className="size-3" />
        <span>Enter to send · Shift+Enter for new line</span>
      </div>
    </div>
  )
}

// ============================================================================
// TaskChat (main component)
// ============================================================================

const TERMINAL_STATUSES = ['done', 'failed']

export function TaskChat({ task, readOnly = false, className }: TaskChatProps) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const bottomRef = useRef<HTMLDivElement>(null)
  const queryClient = useQueryClient()

  const isFailed = task.status === 'failed'
  const isDone = task.status === 'done'
  const isTerminal = TERMINAL_STATUSES.includes(task.status)
  const isDraft = task.status === 'draft'
  const hasHistory = !isDraft
  const canChat = !isDraft && !isTerminal

  // Compute a reason to show when the chat input is disabled
  const disabledReason = isFailed
    ? 'The agent stopped due to an error. You can retry the task from the actions panel.'
    : isDone
      ? 'This task has been completed.'
      : isDraft
        ? undefined // input is hidden entirely for drafts
        : undefined

  // Custom placeholder for plan_review status
  const chatPlaceholder = task.status === 'plan_review'
    ? 'Type to approve the plan and start implementation...'
    : undefined

  // Invalidate task query when SSE reports a status change so the UI refreshes
  const handleStatusChange = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: taskKeys.detail(task.id) })
    queryClient.invalidateQueries({ queryKey: taskKeys.lists() })
  }, [queryClient, task.id])

  const { entries, isConnected, status, addUserMessage } = useTaskChat({
    taskId: task.id,
    enabled: hasHistory,
    taskStatus: task.status,
    onStatusChange: handleStatusChange,
    onComplete: (prUrl) => {
      toast.success('Task completed!', {
        description: prUrl ? `PR: ${prUrl}` : 'Task finished successfully',
      })
    },
    onError: (message) => {
      toast.error('Task failed', { description: message, duration: 8000 })
    },
  })

  // Auto-scroll to bottom on new entries
  useEffect(() => {
    if (bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: 'smooth' })
    }
  }, [entries])

  const renderEntry = (entry: ChatEntry, index: number) => {
    if (entry.type === 'message') {
      return (
        <ChatMessageBubble
          key={(entry.data as ChatMessageEvent).id + '-' + index}
          message={entry.data as ChatMessageEvent}
        />
      )
    }
    return (
      <ToolBadge
        key={(entry.data as ToolActivityEvent).id + '-' + index}
        activity={entry.data as ToolActivityEvent}
      />
    )
  }

  // Determine empty state message
  const emptyStateMessage = isFailed
    ? 'The agent encountered an error before producing any output.'
    : canChat
      ? 'Waiting for agent output...'
      : 'Click Start to begin chatting with the agent'

  return (
    <div className={cn('flex flex-col h-full', className)}>
      {/* Error banner when task has failed, merge conflicts, or canceled */}
      {(task.status === 'failed' || task.status === 'merge_conflicts' || task.status === 'canceled') && task.error && (
        <div className="flex items-start gap-2.5 border-b border-red-500/30 bg-red-950/60 dark:bg-red-50 px-4 py-3 text-sm text-red-300 dark:text-red-700">
          <AlertCircle className="size-4 shrink-0 mt-0.5" />
          <div className="min-w-0 space-y-1">
            <p className="font-medium text-red-200 dark:text-red-800">
              {task.status === 'merge_conflicts'
                ? 'Merge Conflicts'
                : task.status === 'canceled'
                  ? 'Task canceled'
                  : 'Task failed'}
            </p>
            <p className="whitespace-pre-wrap break-words">{task.error}</p>
          </div>
        </div>
      )}

      {/* Chat messages area -- dark bg in light theme, light bg in dark theme */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto bg-zinc-900 dark:bg-zinc-100"
      >
        <div className="p-3 space-y-0.5">
          {entries.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-48 text-zinc-400 dark:text-zinc-500">
              {isFailed ? (
                <AlertCircle className="size-8 mb-3 opacity-40" />
              ) : (
                <MessageSquare className="size-8 mb-3 opacity-40" />
              )}
              <p className="text-sm">{emptyStateMessage}</p>
            </div>
          ) : (
            entries.map(renderEntry)
          )}
          <div ref={bottomRef} />
        </div>
      </div>

      {/* Chat input or disabled reason */}
      {!readOnly && (
        <ChatInput
          taskId={task.id}
          disabled={!canChat}
          disabledReason={disabledReason}
          onMessageSent={addUserMessage}
          placeholder={chatPlaceholder}
        />
      )}
    </div>
  )
}
