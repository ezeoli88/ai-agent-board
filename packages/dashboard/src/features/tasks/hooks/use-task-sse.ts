'use client'

import { useEffect, useCallback, useMemo, useRef, useSyncExternalStore } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { generateId } from '@/lib/utils'
import { getAuthToken } from '@/lib/auth'
import { taskKeys } from './query-keys'
import { useTaskUIStore } from '../stores/task-ui-store'
import type {
  LogEntry,
  TaskStatus,
  SSEConnectionStatus,
  SSELogEvent,
  SSEStatusEvent,
  SSETimeoutWarningEvent,
  SSEAwaitingReviewEvent,
  SSECompleteEvent,
  SSEErrorEvent,
  SSEPRCommentEvent,
  PRComment,
} from '../types'
import type { ChatMessageEvent, ToolActivityEvent } from '@dash-agent/shared'

// Re-export SSEConnectionStatus as ConnectionStatus for backwards compatibility
export type ConnectionStatus = SSEConnectionStatus;

interface UseTaskSSEOptions {
  taskId: string;
  enabled?: boolean;
  streamPath?: string;
  invalidateQueries?: () => void;
  onStatusChange?: (status: TaskStatus) => void;
  onComplete?: (prUrl: string) => void;
  onError?: (message: string) => void;
  onTimeoutWarning?: (message: string, expiresAt: string) => void;
  onPRComment?: (comment: PRComment) => void;
  onChatMessage?: (event: ChatMessageEvent) => void;
  onToolActivity?: (event: ToolActivityEvent) => void;
}

interface ConnectOptions {
  taskId: string;
  enabled: boolean;
  onLog: (log: LogEntry) => void;
  onStatusChange?: (status: TaskStatus) => void;
  onComplete?: (prUrl: string) => void;
  onError?: (message: string) => void;
  onTimeoutWarning?: (message: string, expiresAt: string) => void;
  onPRComment?: (comment: PRComment) => void;
  onChatMessage?: (event: ChatMessageEvent) => void;
  onToolActivity?: (event: ToolActivityEvent) => void;
  invalidateQueries: () => void;
  streamPath?: string;
}

// Connection manager class to handle SSE connections outside of React render cycle
function createSSEConnectionManager() {
  let eventSource: EventSource | null = null;
  let reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  let status: ConnectionStatus = 'disconnected';
  let receivedTerminalEvent = false;
  const listeners = new Set<() => void>();

  function setStatus(newStatus: ConnectionStatus) {
    status = newStatus;
    listeners.forEach(listener => listener());
  }

  function disconnect() {
    if (reconnectTimeout) {
      clearTimeout(reconnectTimeout);
      reconnectTimeout = null;
    }
    if (eventSource) {
      eventSource.close();
      eventSource = null;
    }
    setStatus('disconnected');
  }

  function connect(options: ConnectOptions) {
    const { taskId, enabled, streamPath, onLog, onStatusChange, onComplete, onError, onTimeoutWarning, onPRComment, onChatMessage, onToolActivity, invalidateQueries } = options;

    if (!enabled || !taskId) return;

    // Close existing connection
    disconnect();

    receivedTerminalEvent = false;
    setStatus('connecting');

    const baseUrl = import.meta.env.VITE_API_BASE_URL || window.location.origin;
    const authToken = getAuthToken();
    const path = streamPath ?? `/api/tasks/${taskId}/logs`;
    const tokenParam = authToken
      ? `${path.includes('?') ? '&' : '?'}token=${encodeURIComponent(authToken)}`
      : '';
    const url = `${baseUrl}${path}${tokenParam}`;

    try {
      const es = new EventSource(url);
      eventSource = es;

      es.onopen = () => {
        setStatus('connected');
      };

      // Handle 'log' event - new log entry from agent execution
      es.addEventListener('log', (event) => {
        try {
          const parsed = JSON.parse(event.data) as SSELogEvent['data'];
          onLog({
            id: generateId(),
            timestamp: parsed.timestamp,
            level: parsed.level,
            message: parsed.message,
            data: parsed.data,
          });
        } catch (e) {
          console.error('Failed to parse log event:', e);
        }
      });

      // Handle 'status' event - task status changed
      es.addEventListener('status', (event) => {
        try {
          const parsed = JSON.parse(event.data) as SSEStatusEvent['data'];
          onStatusChange?.(parsed.status);
          invalidateQueries();
        } catch (e) {
          console.error('Failed to parse status event:', e);
        }
      });

      // Handle 'timeout_warning' event - agent is about to timeout
      es.addEventListener('timeout_warning', (event) => {
        try {
          const parsed = JSON.parse(event.data) as SSETimeoutWarningEvent['data'];
          onTimeoutWarning?.(parsed.message, parsed.expires_at);
        } catch (e) {
          console.error('Failed to parse timeout_warning event:', e);
        }
      });

      // Handle 'awaiting_review' event - agent needs user review
      es.addEventListener('awaiting_review', (event) => {
        try {
          const parsed = JSON.parse(event.data) as SSEAwaitingReviewEvent['data'];
          // Add as a special log entry
          onLog({
            id: generateId(),
            timestamp: new Date().toISOString(),
            level: 'agent',
            message: parsed.message,
          });
          invalidateQueries();
        } catch (e) {
          console.error('Failed to parse awaiting_review event:', e);
        }
      });

      // Handle 'complete' event - task execution completed successfully
      es.addEventListener('complete', (event) => {
        try {
          const parsed = JSON.parse(event.data) as SSECompleteEvent['data'];
          receivedTerminalEvent = true;
          onComplete?.(parsed.pr_url ?? '');
          invalidateQueries();
          es.close();
          setStatus('disconnected');
        } catch (e) {
          console.error('Failed to parse complete event:', e);
        }
      });

      // Handle 'error' event - task execution failed
      es.addEventListener('error', (event) => {
        try {
          const parsed = JSON.parse((event as MessageEvent).data) as SSEErrorEvent['data'];
          receivedTerminalEvent = true;
          // Don't trigger onError for cancelled tasks - the cancel action already shows a toast
          if (parsed.code !== 'CANCELLED') {
            onError?.(parsed.message);
          }
          // Invalidate queries so task.error is fetched and displayed in the UI
          invalidateQueries();
        } catch {
          // Connection error, not a data error - handled by onerror
        }
      });

      // Handle 'pr_comment' event - new comment on PR
      es.addEventListener('pr_comment', (event) => {
        try {
          const parsed = JSON.parse(event.data) as SSEPRCommentEvent['data'];
          onPRComment?.(parsed.comment);
        } catch (e) {
          console.error('Failed to parse pr_comment event:', e);
        }
      });

      // Handle 'chat_message' event - chat message from agent or user
      es.addEventListener('chat_message', (event: MessageEvent) => {
        try {
          const data = JSON.parse(event.data) as ChatMessageEvent;
          onChatMessage?.(data);
          // Also add assistant messages to the log stream
          if (data.role === 'assistant' && data.content) {
            onLog({
              id: generateId(),
              timestamp: data.timestamp || new Date().toISOString(),
              level: 'agent',
              message: data.content,
            });
          }
        } catch { /* ignore */ }
      });

      // Handle 'tool_activity' event - tool call badge
      es.addEventListener('tool_activity', (event: MessageEvent) => {
        try {
          const data = JSON.parse(event.data) as ToolActivityEvent;
          onToolActivity?.(data);
        } catch { /* ignore */ }
      });

      es.onerror = () => {
        es.close();
        // Don't reconnect if we received a terminal event (complete/error data)
        // — the server closed the connection intentionally after sending final data
        if (receivedTerminalEvent) {
          setStatus('disconnected');
          return;
        }
        setStatus('error');
        // Auto-reconnect after 3 seconds
        reconnectTimeout = setTimeout(() => {
          if (enabled) connect(options);
        }, 3000);
      };
    } catch (e) {
      console.error('Failed to create EventSource:', e);
      setStatus('error');
    }
  }

  return {
    connect,
    disconnect,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    getSnapshot: () => status,
  };
}

export function useTaskSSE(options: UseTaskSSEOptions) {
  const { taskId, enabled = true, streamPath, invalidateQueries, onStatusChange, onComplete, onError, onTimeoutWarning, onPRComment, onChatMessage, onToolActivity } = options;
  const queryClient = useQueryClient();

  // Use Zustand store for logs persistence across tab switches
  const { taskLogs, addTaskLog, clearTaskLogs } = useTaskUIStore();
  const logs = taskLogs[taskId] || [];

  // Store callbacks in a ref so SSE event handlers always call the latest version
  // without causing the connectOptions memo (and thus the effect) to re-run.
  const callbacksRef = useRef({ onStatusChange, onComplete, onError, onTimeoutWarning, onPRComment, onChatMessage, onToolActivity });
  callbacksRef.current = { onStatusChange, onComplete, onError, onTimeoutWarning, onPRComment, onChatMessage, onToolActivity };

  // Create stable manager instance using useMemo
  const manager = useMemo(() => createSSEConnectionManager(), []);

  // Use useSyncExternalStore to subscribe to connection status
  const connectionStatus = useSyncExternalStore(
    manager.subscribe,
    manager.getSnapshot,
    () => 'disconnected' as ConnectionStatus
  );

  // Memoize connect options — only depends on stable values (taskId, enabled,
  // queryClient, addTaskLog). Callbacks are accessed via callbacksRef so they
  // never trigger a reconnection when callers pass new arrow functions.
  const connectOptions = useMemo((): ConnectOptions => ({
    taskId,
    enabled,
    streamPath,
    onLog: (log) => addTaskLog(taskId, log),
    onStatusChange: (status) => callbacksRef.current.onStatusChange?.(status),
    onComplete: (prUrl) => callbacksRef.current.onComplete?.(prUrl),
    onError: (message) => callbacksRef.current.onError?.(message),
    onTimeoutWarning: (message, expiresAt) => callbacksRef.current.onTimeoutWarning?.(message, expiresAt),
    onPRComment: (comment) => callbacksRef.current.onPRComment?.(comment),
    onChatMessage: (event) => callbacksRef.current.onChatMessage?.(event),
    onToolActivity: (event) => callbacksRef.current.onToolActivity?.(event),
    invalidateQueries: invalidateQueries ?? (() => {
      queryClient.invalidateQueries({ queryKey: taskKeys.detail(taskId) });
      queryClient.invalidateQueries({ queryKey: taskKeys.lists() });
      queryClient.invalidateQueries({ queryKey: taskKeys.changes(taskId) });
    }),
  }), [taskId, enabled, streamPath, queryClient, addTaskLog, invalidateQueries]);

  // Keep a ref to connectOptions so reconnect() always uses the latest value
  const connectOptionsRef = useRef(connectOptions);
  connectOptionsRef.current = connectOptions;

  // Connect/disconnect effect
  useEffect(() => {
    if (!enabled || !taskId) {
      manager.disconnect();
      return;
    }

    manager.connect(connectOptions);

    return () => {
      manager.disconnect();
    };
  }, [taskId, enabled, manager, connectOptions]);

  const clearLogs = useCallback(() => {
    clearTaskLogs(taskId);
  }, [taskId, clearTaskLogs]);

  const addLog = useCallback((entry: Omit<LogEntry, 'id'>) => {
    addTaskLog(taskId, {
      id: generateId(),
      ...entry
    });
  }, [taskId, addTaskLog]);

  const reconnect = useCallback(() => {
    manager.connect(connectOptionsRef.current);
  }, [manager]);

  const disconnect = useCallback(() => {
    manager.disconnect();
  }, [manager]);

  return {
    logs,
    connectionStatus,
    clearLogs,
    addLog,
    reconnect,
    disconnect,
  };
}
