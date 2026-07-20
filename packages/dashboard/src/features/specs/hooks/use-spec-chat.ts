'use client'

import { useCallback, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { specsApi } from '@/lib/api-client'
import { generateId } from '@/lib/utils'
import { useTaskSSE } from '@/features/tasks/hooks/use-task-sse'
import type { ChatMessageEvent, ToolActivityEvent } from '@dash-agent/shared'
import { specKeys } from './query-keys'

export interface SpecChatEntry {
  type: 'message' | 'tool'
  data: ChatMessageEvent | ToolActivityEvent
}

export function useSpecChat(specId: string, enabled = true) {
  const queryClient = useQueryClient()
  const [entries, setEntries] = useState<SpecChatEntry[]>([])

  const handleChatMessage = useCallback((event: ChatMessageEvent) => {
    setEntries((prev) => [...prev, { type: 'message', data: event }])
  }, [])

  const handleToolActivity = useCallback((event: ToolActivityEvent) => {
    setEntries((prev) => {
      const existingIndex = prev.findIndex(
        (entry) => entry.type === 'tool' && (entry.data as ToolActivityEvent).id === event.id && event.id !== ''
      )
      if (existingIndex >= 0) {
        const updated = [...prev]
        const existing = prev[existingIndex].data as ToolActivityEvent
        updated[existingIndex] = {
          type: 'tool',
          data: {
            ...existing,
            status: event.status,
            name: event.name || existing.name,
            summary: event.status === 'error' ? (event.summary || existing.summary) : existing.summary,
          },
        }
        return updated
      }
      return [...prev, { type: 'tool', data: event }]
    })
  }, [])

  const invalidateSpec = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: specKeys.detail(specId) })
    queryClient.invalidateQueries({ queryKey: specKeys.lists() })
    queryClient.invalidateQueries({ queryKey: specKeys.agentStatus(specId) })
  }, [queryClient, specId])

  const sse = useTaskSSE({
    taskId: specId,
    enabled,
    streamPath: `/api/specs/${specId}/logs`,
    invalidateQueries: invalidateSpec,
    onChatMessage: handleChatMessage,
    onToolActivity: handleToolActivity,
    onError: (message) => {
      toast.error('Spec agent failed', { description: message, duration: 8000 })
    },
  })

  const sendMessage = useMutation({
    mutationFn: (content: string) => specsApi.sendFeedback(specId, content),
    onSuccess: (_data, content) => {
      const event: ChatMessageEvent = {
        id: generateId(),
        role: 'user',
        content,
        timestamp: new Date().toISOString(),
      }
      setEntries((prev) => [...prev, { type: 'message', data: event }])
      queryClient.invalidateQueries({ queryKey: specKeys.agentStatus(specId) })
    },
    onError: (error: Error) => {
      toast.error('Could not send message', { description: error.message })
    },
  })

  return {
    entries,
    logs: sse.logs,
    connectionStatus: sse.connectionStatus,
    isConnected: sse.connectionStatus === 'connected',
    sendMessage,
  }
}
