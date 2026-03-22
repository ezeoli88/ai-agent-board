'use client'

import { useEffect, useRef, useState, useCallback } from 'react'
import {
  MessageSquare,
  Terminal,
  Loader2,
  Send,
  Keyboard,
  AlertCircle,
  ChevronDown,
  Eye,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { ChatMessageBubble, ToolBadge } from '@/components/shared/chat'
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '@/components/ui/collapsible'
import { cn } from '@/lib/utils'
import { generateId } from '@/lib/utils'
import { useSpecSSE } from '../hooks/use-spec-sse'
import { useSpecFeedback } from '../hooks/use-spec-actions'
import type { Spec } from '../types'
import type { ChatMessageEvent, ToolActivityEvent } from '@dash-agent/shared'
import type { SpecLogEntry } from '../hooks/use-spec-sse'

interface SpecChatProps {
  spec: Spec
  className?: string
  /** Callback when the user wants to navigate to the spec preview tab */
  onViewSpec?: () => void
}

// ============================================================================
// LogsSection (collapsible)
// ============================================================================

function LogsSection({ logs }: { logs: SpecLogEntry[] }) {
  const [isOpen, setIsOpen] = useState(false)
  const logsEndRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (isOpen && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: 'smooth' })
    }
  }, [logs, isOpen])

  if (logs.length === 0) return null

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <CollapsibleTrigger asChild>
        <button
          type="button"
          className="flex items-center gap-1.5 text-xs text-zinc-400 dark:text-zinc-500 hover:text-zinc-300 dark:hover:text-zinc-400 transition-colors py-1"
        >
          <ChevronDown
            className={cn(
              'size-3 transition-transform',
              isOpen && 'rotate-180'
            )}
          />
          <Terminal className="size-3" />
          <span>Agent logs ({logs.length})</span>
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="mt-1 max-h-48 overflow-y-auto rounded bg-zinc-950 dark:bg-zinc-50 border border-zinc-800 dark:border-zinc-200">
          <div className="p-2 space-y-0.5">
            {logs.map((entry) => (
              <div
                key={entry.id}
                className="flex items-start gap-2 text-xs font-mono"
              >
                <span className="text-zinc-600 dark:text-zinc-400 shrink-0 tabular-nums">
                  {formatTimestamp(entry.timestamp)}
                </span>
                <span
                  className={cn(
                    'break-all',
                    entry.level === 'error'
                      ? 'text-red-400 dark:text-red-600'
                      : entry.level === 'warn'
                        ? 'text-amber-400 dark:text-amber-600'
                        : entry.level === 'agent'
                          ? 'text-purple-300 dark:text-purple-600'
                          : 'text-zinc-400 dark:text-zinc-500'
                  )}
                >
                  {entry.message}
                </span>
              </div>
            ))}
            <div ref={logsEndRef} />
          </div>
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

// ============================================================================
// ChatInput
// ============================================================================

function ChatInput({
  specId,
  disabled,
  disabledReason,
  onMessageSent,
}: {
  specId: string
  disabled: boolean
  disabledReason?: string
  onMessageSent?: (content: string) => void
}) {
  const [message, setMessage] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const specFeedback = useSpecFeedback()

  const canSubmit = message.trim().length > 0 && !specFeedback.isPending && !disabled

  const handleSubmit = useCallback(() => {
    if (!canSubmit) return
    const trimmed = message.trim()
    onMessageSent?.(trimmed)
    setMessage('')
    specFeedback.mutate(
      { id: specId, message: trimmed },
      {
        onSuccess: () => {
          textareaRef.current?.focus()
        },
      }
    )
  }, [canSubmit, message, specFeedback, specId, onMessageSent])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault()
        handleSubmit()
      }
    },
    [handleSubmit]
  )

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
          placeholder="Send feedback to refine the spec..."
          disabled={specFeedback.isPending}
          className="min-h-[40px] max-h-[120px] resize-none text-sm"
          rows={1}
        />
        <Button
          size="icon"
          onClick={handleSubmit}
          disabled={!canSubmit}
          className="shrink-0 self-end"
        >
          {specFeedback.isPending ? (
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
// Helpers
// ============================================================================

function formatTimestamp(timestamp: string): string {
  try {
    const date = new Date(timestamp)
    return date.toLocaleTimeString('en-US', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    })
  } catch {
    return timestamp
  }
}

// ============================================================================
// ChatEntry type (local to this component, mirrors task-chat pattern)
// ============================================================================

interface ChatEntry {
  type: 'message' | 'tool'
  data: ChatMessageEvent | ToolActivityEvent
}

// ============================================================================
// SpecChat (main component)
// ============================================================================

const TERMINAL_STATUSES = ['ready', 'failed', 'cancelled']

export function SpecChat({ spec, className, onViewSpec }: SpecChatProps) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const bottomRef = useRef<HTMLDivElement>(null)
  const [entries, setEntries] = useState<ChatEntry[]>([])

  const isTerminal = TERMINAL_STATUSES.includes(spec.status)
  const canChat = spec.status === 'draft' || spec.status === 'generating'

  const disabledReason = isTerminal
    ? spec.status === 'ready'
      ? 'This spec has been approved and sent to coding.'
      : spec.status === 'failed'
        ? 'The agent encountered an error.'
        : 'This spec has been cancelled.'
    : undefined

  // SSE connection: only connect if the spec is actively being refined
  const { logs, chatMessages, toolActivities, isAgentRunning } = useSpecSSE({
    specId: spec.id,
    enabled: spec.status === 'generating',
  })

  // Build interleaved entries from SSE chat messages and tool activities
  useEffect(() => {
    const messageEntries: ChatEntry[] = chatMessages.map((m) => ({
      type: 'message' as const,
      data: m,
    }))

    const toolEntries: ChatEntry[] = toolActivities.map((t) => ({
      type: 'tool' as const,
      data: t,
    }))

    // Merge by timestamp to maintain chronological order
    const all = [...messageEntries, ...toolEntries].sort((a, b) => {
      const tsA = 'timestamp' in a.data ? a.data.timestamp : ''
      const tsB = 'timestamp' in b.data ? b.data.timestamp : ''
      return tsA.localeCompare(tsB)
    })

    setEntries(all)
  }, [chatMessages, toolActivities])

  // Auto-scroll to bottom on new entries
  useEffect(() => {
    if (bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: 'smooth' })
    }
  }, [entries, logs])

  // Optimistic user message handler
  const addUserMessage = useCallback((content: string) => {
    const event: ChatMessageEvent = {
      id: generateId(),
      role: 'user',
      content,
      timestamp: new Date().toISOString(),
    }
    setEntries((prev) => [...prev, { type: 'message', data: event }])
  }, [])

  const renderEntry = (entry: ChatEntry, index: number) => {
    if (entry.type === 'message') {
      const msg = entry.data as ChatMessageEvent
      return <ChatMessageBubble key={`${msg.id}-${index}`} message={msg} />
    }
    const tool = entry.data as ToolActivityEvent
    return <ToolBadge key={`${tool.id}-${index}`} activity={tool} />
  }

  const showViewSpec = !!spec.draft_spec && !isAgentRunning
  const emptyStateMessage = isAgentRunning
    ? 'Waiting for agent output...'
    : canChat
      ? 'Start refining to chat with the agent'
      : 'No chat history'

  return (
    <div className={cn('flex flex-col h-full', className)}>
      {/* Error banner */}
      {spec.status === 'failed' && (
        <div className="flex items-start gap-2.5 border-b border-red-500/30 bg-red-950/60 dark:bg-red-50 px-4 py-3 text-sm text-red-300 dark:text-red-700">
          <AlertCircle className="size-4 shrink-0 mt-0.5" />
          <div className="min-w-0">
            <p className="font-medium text-red-200 dark:text-red-800">
              Agent failed
            </p>
          </div>
        </div>
      )}

      {/* Activity indicator when agent is running */}
      {isAgentRunning && (
        <div className="flex items-center gap-2 border-b px-4 py-2 bg-blue-500/10">
          <Loader2 className="size-3.5 animate-spin text-blue-500" />
          <span className="text-xs text-blue-500 font-medium">
            Agent is working...
          </span>
        </div>
      )}

      {/* Chat messages area */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto bg-zinc-900 dark:bg-zinc-100"
      >
        <div className="p-3 space-y-0.5">
          {entries.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-48 text-zinc-400 dark:text-zinc-500">
              <MessageSquare className="size-8 mb-3 opacity-40" />
              <p className="text-sm">{emptyStateMessage}</p>
            </div>
          ) : (
            entries.map(renderEntry)
          )}

          {/* Collapsible logs section */}
          <LogsSection logs={logs} />

          <div ref={bottomRef} />
        </div>
      </div>

      {/* "View Spec" link when draft is ready and agent is idle */}
      {showViewSpec && onViewSpec && (
        <div className="border-t bg-muted/30 px-4 py-2.5">
          <button
            type="button"
            onClick={onViewSpec}
            className="flex items-center gap-1.5 text-sm font-medium text-blue-500 hover:text-blue-400 dark:text-blue-400 dark:hover:text-blue-300 transition-colors"
          >
            <Eye className="size-3.5" />
            View Spec
            <span aria-hidden="true">&rarr;</span>
          </button>
        </div>
      )}

      {/* Chat input */}
      <ChatInput
        specId={spec.id}
        disabled={!canChat}
        disabledReason={disabledReason}
        onMessageSent={addUserMessage}
      />
    </div>
  )
}
