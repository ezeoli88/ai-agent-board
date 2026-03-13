'use client'

import { useEffect, useCallback, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { generateId } from '@/lib/utils'
import { getAuthToken } from '@/lib/auth'
import { specKeys } from './query-keys'
import type { ChatMessageEvent, ToolActivityEvent } from '@dash-agent/shared'

export type SpecSSEConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'error'

export interface SpecLogEntry {
  id: string
  timestamp: string
  level: string
  message: string
  data?: unknown
}

interface UseSpecSSEOptions {
  specId: string
  enabled?: boolean
}

interface UseSpecSSEReturn {
  logs: SpecLogEntry[]
  chatMessages: ChatMessageEvent[]
  toolActivities: ToolActivityEvent[]
  connectionStatus: SpecSSEConnectionStatus
  isAgentRunning: boolean
  clearLogs: () => void
}

export function useSpecSSE({ specId, enabled = true }: UseSpecSSEOptions): UseSpecSSEReturn {
  const queryClient = useQueryClient()
  const [logs, setLogs] = useState<SpecLogEntry[]>([])
  const [chatMessages, setChatMessages] = useState<ChatMessageEvent[]>([])
  const [toolActivities, setToolActivities] = useState<ToolActivityEvent[]>([])
  const [connectionStatus, setConnectionStatus] = useState<SpecSSEConnectionStatus>('disconnected')
  const [isAgentRunning, setIsAgentRunning] = useState(false)

  const eventSourceRef = useRef<EventSource | null>(null)
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const receivedTerminalEventRef = useRef(false)

  const invalidateSpecQueries = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: specKeys.detail(specId) })
    queryClient.invalidateQueries({ queryKey: specKeys.lists() })
  }, [queryClient, specId])

  const clearLogs = useCallback(() => {
    setLogs([])
    setChatMessages([])
    setToolActivities([])
  }, [])

  useEffect(() => {
    if (!enabled || !specId) {
      setConnectionStatus('disconnected')
      return
    }

    function disconnect() {
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current)
        reconnectTimeoutRef.current = null
      }
      if (eventSourceRef.current) {
        eventSourceRef.current.close()
        eventSourceRef.current = null
      }
    }

    function connect() {
      disconnect()

      receivedTerminalEventRef.current = false
      setConnectionStatus('connecting')

      const baseUrl = import.meta.env.VITE_API_BASE_URL || window.location.origin
      const authToken = getAuthToken()
      const tokenParam = authToken ? `?token=${encodeURIComponent(authToken)}` : ''
      const url = `${baseUrl}/api/specs/${specId}/logs${tokenParam}`

      try {
        const es = new EventSource(url)
        eventSourceRef.current = es

        es.onopen = () => {
          setConnectionStatus('connected')
          setIsAgentRunning(true)
        }

        // Handle 'log' event - new log entry from agent execution
        es.addEventListener('log', (event) => {
          try {
            const parsed = JSON.parse(event.data) as { timestamp: string; level: string; message: string; data?: unknown }
            setLogs(prev => [...prev, {
              id: generateId(),
              timestamp: parsed.timestamp,
              level: parsed.level,
              message: parsed.message,
              data: parsed.data,
            }])
          } catch (e) {
            console.error('Failed to parse spec log event:', e)
          }
        })

        // Handle 'status' event - spec status changed
        es.addEventListener('status', (event) => {
          try {
            const parsed = JSON.parse(event.data) as { status: string }
            // Spec is no longer running if status is terminal
            if (['approved', 'failed', 'canceled'].includes(parsed.status)) {
              setIsAgentRunning(false)
            }
            invalidateSpecQueries()
          } catch (e) {
            console.error('Failed to parse spec status event:', e)
          }
        })

        // Handle 'chat_message' event - chat message from agent or user
        es.addEventListener('chat_message', (event: MessageEvent) => {
          try {
            const data = JSON.parse(event.data) as ChatMessageEvent
            setChatMessages(prev => [...prev, data])
            // Also add assistant messages to the log stream for visibility
            if (data.role === 'assistant' && data.content) {
              setLogs(prev => [...prev, {
                id: generateId(),
                timestamp: data.timestamp || new Date().toISOString(),
                level: 'agent',
                message: data.content,
              }])
            }
          } catch { /* ignore parse errors */ }
        })

        // Handle 'spec_ready' event - draft_spec has been updated
        es.addEventListener('spec_ready', () => {
          invalidateSpecQueries()
        })

        // Handle 'error' event - spec agent execution failed
        es.addEventListener('error', (event) => {
          try {
            const parsed = JSON.parse((event as MessageEvent).data) as { message: string; code?: string }
            receivedTerminalEventRef.current = true
            setIsAgentRunning(false)
            // Add error to logs so user can see what went wrong
            setLogs(prev => [...prev, {
              id: generateId(),
              timestamp: new Date().toISOString(),
              level: 'error',
              message: parsed.message,
            }])
            invalidateSpecQueries()
          } catch {
            // Connection error, not a data error - handled by onerror
          }
        })

        // Handle 'tool_activity' event - tool call badge
        es.addEventListener('tool_activity', (event: MessageEvent) => {
          try {
            const data = JSON.parse(event.data) as ToolActivityEvent
            setToolActivities(prev => [...prev, data])
          } catch { /* ignore parse errors */ }
        })

        // Handle 'complete' event - agent finished
        es.addEventListener('complete', () => {
          receivedTerminalEventRef.current = true
          setIsAgentRunning(false)
          invalidateSpecQueries()
          es.close()
          setConnectionStatus('disconnected')
        })

        es.onerror = () => {
          es.close()
          // Don't reconnect if we received a terminal event
          if (receivedTerminalEventRef.current) {
            setConnectionStatus('disconnected')
            setIsAgentRunning(false)
            return
          }
          setConnectionStatus('error')
          // Auto-reconnect after 3 seconds
          reconnectTimeoutRef.current = setTimeout(() => {
            if (enabled) connect()
          }, 3000)
        }
      } catch (e) {
        console.error('Failed to create spec EventSource:', e)
        setConnectionStatus('error')
      }
    }

    connect()

    return () => {
      disconnect()
      setConnectionStatus('disconnected')
    }
  }, [specId, enabled, invalidateSpecQueries])

  return {
    logs,
    chatMessages,
    toolActivities,
    connectionStatus,
    isAgentRunning,
    clearLogs,
  }
}
