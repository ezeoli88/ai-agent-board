import { useEffect, useRef } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { getAuthToken } from '@/lib/auth'
import { taskKeys } from '@/features/tasks/hooks/query-keys'
import { repoKeys } from '@/features/repos/hooks/query-keys'
import { specKeys } from '@/features/specs/hooks/query-keys'

/**
 * Data change event pushed by the server when tasks or repos are mutated.
 * Matches the DataChangeEvent interface on the backend.
 */
interface DataChangeEvent {
  entity: 'task' | 'repo' | 'spec' | 'qa_run'
  action: 'created' | 'updated' | 'deleted'
  id?: string
}

/**
 * Connects to the global /api/events SSE endpoint and invalidates
 * TanStack Query caches when the server pushes data-change events.
 *
 * This ensures the dashboard reflects changes made via the API
 * (e.g., from an MCP client like Claude Code) without polling.
 *
 * Reconnection is handled automatically by the EventSource API.
 * If the connection fails, EventSource retries with an exponential backoff.
 */
export function useDataInvalidation() {
  const queryClient = useQueryClient()
  // Track the EventSource so we can close on cleanup even if queryClient changes
  const eventSourceRef = useRef<EventSource | null>(null)

  useEffect(() => {
    const baseUrl = import.meta.env.VITE_API_BASE_URL || window.location.origin
    const authToken = getAuthToken()
    const tokenParam = authToken ? `?token=${encodeURIComponent(authToken)}` : ''
    const url = `${baseUrl}/api/events${tokenParam}`

    const es = new EventSource(url)
    eventSourceRef.current = es

    es.addEventListener('data-change', (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data) as DataChangeEvent

        if (data.entity === 'task') {
          // Invalidate all task list queries (any filter combination)
          queryClient.invalidateQueries({ queryKey: taskKeys.lists() })
          if (data.id) {
            queryClient.invalidateQueries({ queryKey: taskKeys.detail(data.id) })
            queryClient.invalidateQueries({ queryKey: taskKeys.changes(data.id) })
          }
        } else if (data.entity === 'qa_run') {
          queryClient.invalidateQueries({ queryKey: taskKeys.qaBoards() })
          queryClient.invalidateQueries({ queryKey: taskKeys.all })
        } else if (data.entity === 'repo') {
          // Invalidate all repo queries
          queryClient.invalidateQueries({ queryKey: repoKeys.lists() })
          if (data.id) {
            queryClient.invalidateQueries({ queryKey: repoKeys.detail(data.id) })
          }
        } else if (data.entity === 'spec') {
          queryClient.invalidateQueries({ queryKey: specKeys.lists() })
          if (data.id) {
            queryClient.invalidateQueries({ queryKey: specKeys.detail(data.id) })
          }
        }
      } catch {
        // Ignore malformed events
      }
    })

    es.onerror = () => {
      // EventSource automatically reconnects on error.
      // No action needed here; logging is optional for debugging.
    }

    return () => {
      es.close()
      eventSourceRef.current = null
    }
  }, [queryClient])
}
