'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import {
  AlertCircle,
  Check,
  FileText,
  Keyboard,
  Loader2,
  MessageSquare,
  Send,
  Terminal,
  X,
} from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { useRolePermissions } from '@/features/settings'
import { cn } from '@/lib/utils'
import { useSpecAgentStatus } from '../hooks/use-spec-agent-status'
import { useSpecChat, type SpecChatEntry } from '../hooks/use-spec-chat'
import type { Spec } from '../types'
import type { ChatMessageEvent, ToolActivityEvent } from '@dash-agent/shared'

interface SpecChatProps {
  spec: Spec
  className?: string
}

function ChatMessageBubble({ message }: { message: ChatMessageEvent }) {
  if (message.role === 'user') {
    return (
      <div className="flex justify-end py-1.5">
        <div className="max-w-[85%] rounded-md bg-blue-600 px-3 py-2 text-white">
          <p className="whitespace-pre-wrap break-words text-sm">{message.content}</p>
        </div>
      </div>
    )
  }

  if (message.role === 'assistant') {
    return (
      <div className="flex justify-start py-1.5">
        <div className="max-w-[85%] rounded-md bg-zinc-800 px-3 py-2 text-zinc-100 dark:bg-zinc-200 dark:text-zinc-900">
          <p className="whitespace-pre-wrap break-words text-sm">{message.content}</p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex justify-center py-1.5">
      <span className="text-xs italic text-zinc-400 dark:text-zinc-500">
        {message.content}
      </span>
    </div>
  )
}

function ToolBadge({ activity }: { activity: ToolActivityEvent }) {
  const displayName = activity.name || 'Tool'
  return (
    <div className="flex items-center py-0.5">
      <Badge
        variant="secondary"
        className="max-w-full gap-1.5 border-0 bg-zinc-800 px-2 py-0.5 text-xs font-normal text-zinc-300 dark:bg-zinc-200 dark:text-zinc-600"
      >
        {activity.status === 'running' ? (
          <Loader2 className="size-3 shrink-0 animate-spin text-blue-400 dark:text-blue-600" />
        ) : activity.status === 'completed' ? (
          <Check className="size-3 shrink-0 text-emerald-400 dark:text-emerald-600" />
        ) : activity.status === 'error' ? (
          <X className="size-3 shrink-0 text-red-400 dark:text-red-600" />
        ) : (
          <Terminal className="size-3 shrink-0" />
        )}
        <span className="font-medium text-zinc-200 dark:text-zinc-700">{displayName}</span>
        {activity.summary && (
          <span className="max-w-[300px] truncate text-zinc-400 dark:text-zinc-500">
            {activity.summary}
          </span>
        )}
      </Badge>
    </div>
  )
}

function LogLine({ level, message }: { level: string; message: string }) {
  const isError = level === 'error'
  const isWarn = level === 'warn'
  return (
    <div className="flex items-start gap-2 rounded-sm px-2 py-1 font-mono text-xs text-zinc-400 dark:text-zinc-600">
      {isError ? (
        <AlertCircle className="mt-0.5 size-3 shrink-0 text-red-400" />
      ) : isWarn ? (
        <AlertCircle className="mt-0.5 size-3 shrink-0 text-amber-400" />
      ) : (
        <Terminal className="mt-0.5 size-3 shrink-0" />
      )}
      <span className="shrink-0 uppercase">{level}</span>
      <span className="min-w-0 whitespace-pre-wrap break-words">{message}</span>
    </div>
  )
}

function SpecChatInput({
  disabled,
  disabledReason,
  isSending,
  onSend,
}: {
  disabled: boolean
  disabledReason?: string
  isSending: boolean
  onSend: (message: string) => void
}) {
  const [message, setMessage] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const canSubmit = message.trim().length > 0 && !disabled && !isSending

  const handleSubmit = useCallback(() => {
    if (!canSubmit) return
    onSend(message.trim())
    setMessage('')
    textareaRef.current?.focus()
  }, [canSubmit, message, onSend])

  const handleKeyDown = useCallback((event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      handleSubmit()
    }
  }, [handleSubmit])

  if (disabled) {
    return (
      <div className="border-t bg-background px-4 py-3">
        <p className="text-center text-sm text-muted-foreground">{disabledReason}</p>
      </div>
    )
  }

  return (
    <div className="space-y-2 border-t bg-background p-3">
      <div className="flex gap-2">
        <Textarea
          ref={textareaRef}
          value={message}
          onChange={(event) => setMessage(event.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Send a message to the spec agent..."
          disabled={isSending}
          className="max-h-[120px] min-h-[40px] resize-none text-sm"
          rows={1}
        />
        <Button
          size="icon"
          onClick={handleSubmit}
          disabled={!canSubmit}
          className="shrink-0 self-end"
        >
          {isSending ? <Loader2 className="size-4 animate-spin" /> : <Send className="size-4" />}
        </Button>
      </div>
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Keyboard className="size-3" />
        <span>Enter to send / Shift+Enter for new line</span>
      </div>
    </div>
  )
}

export function SpecChat({ spec, className }: SpecChatProps) {
  const scrollContainerRef = useRef<HTMLDivElement>(null)
  const { canManageSpecs } = useRolePermissions()
  const { data: agentStatus } = useSpecAgentStatus(spec.id)
  const { entries, logs, connectionStatus, sendMessage } = useSpecChat(spec.id)
  const isRunning = agentStatus?.running ?? false
  const isSpecChatPhase = spec.status === 'clarifying' || spec.status === 'spec_draft'
  const canChat = canManageSpecs && (isRunning || isSpecChatPhase)

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const container = scrollContainerRef.current
      if (!container) return
      container.scrollTo({ top: container.scrollHeight, behavior: 'smooth' })
    })

    return () => window.cancelAnimationFrame(frame)
  }, [entries.length, logs.length])

  const renderEntry = (entry: SpecChatEntry, index: number) => {
    if (entry.type === 'message') {
      const message = entry.data as ChatMessageEvent
      return <ChatMessageBubble key={`${message.id}-${index}`} message={message} />
    }
    const tool = entry.data as ToolActivityEvent
    return <ToolBadge key={`${tool.id}-${index}`} activity={tool} />
  }

  const hasOutput = entries.length > 0 || logs.length > 0
  const disabledReason = canChat
    ? undefined
    : canManageSpecs
      ? 'Chat is available while clarifying or reviewing the generated spec draft.'
      : 'Read-only access for DEV role.'

  return (
    <div className={cn('flex h-full min-h-0 flex-col', className)}>
      <div className="flex items-center justify-between border-b bg-background px-3 py-2">
        <div className="flex items-center gap-2">
          <MessageSquare className="size-4 text-muted-foreground" />
          <span className="text-sm font-medium">Agent Output</span>
        </div>
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <span
            className={cn(
              'size-2 rounded-full',
              connectionStatus === 'connected'
                ? 'bg-green-500'
                : connectionStatus === 'connecting'
                  ? 'animate-pulse bg-amber-500'
                  : 'bg-muted-foreground'
            )}
          />
          {isRunning ? 'Running' : spec.status === 'clarifying' ? 'Clarifying' : spec.status === 'spec_draft' ? 'Spec chat' : 'Idle'}
        </div>
      </div>

      <div
        ref={scrollContainerRef}
        className="min-h-0 flex-1 overflow-y-auto overscroll-contain bg-zinc-900 dark:bg-zinc-100"
      >
        <div className="space-y-0.5 p-3">
          {!hasOutput ? (
            <div className="flex h-48 flex-col items-center justify-center text-zinc-400 dark:text-zinc-500">
              <FileText className="mb-3 size-8 opacity-40" />
              <p className="text-sm">Generate a spec, plan, or breakdown to see agent output here.</p>
            </div>
          ) : (
            <>
              {entries.map(renderEntry)}
              {logs.map((log) => (
                <LogLine key={log.id} level={log.level} message={log.message} />
              ))}
            </>
          )}
        </div>
      </div>

      <SpecChatInput
        disabled={!canChat}
        disabledReason={disabledReason}
        isSending={sendMessage.isPending}
        onSend={(message) => sendMessage.mutate(message)}
      />
    </div>
  )
}
