/**
 * Task Types - Re-exports from @dash-agent/shared
 *
 * This file re-exports shared types and adds frontend-specific utilities.
 * The single source of truth for types is @dash-agent/shared.
 */

// ============================================================================
// Core Task Types (from @dash-agent/shared)
// ============================================================================

export {
  // Task schemas and types
  TASK_STATUSES,
  QA_RUN_STATUSES,
  type TaskStatus,
  type QaRunStatus,
  type QaRun,
  type CreateQaRunInput,
  type Task,
  type QaBoardItem,
  type CreateTaskInput,
  type UpdateTaskInput,
  // Two-agent workflow types
  type GenerateSpecRequest,
  type GenerateSpecResponse,
  type UpdateSpecRequest,
  type ApproveSpecRequest,
  type ApproveSpecResponse,
  // Task utilities
  TASK_STATUS_LABELS,
  TASK_STATUS_COLORS,
  getAvailableActionsForStatus,
  isTerminalStatus,
  isActiveStatus,
  requiresUserAction,
  // New two-agent workflow utilities
  isSpecPhase,
  isCodingPhase,
  mapLegacyStatus,
  getPhaseForStatus,
  // SSE types
  type LogLevel,
  type LogEntry,
  type SSEEventType,
  type SSELogEvent,
  type SSEStatusEvent,
  type SSETimeoutWarningEvent,
  type SSEAwaitingReviewEvent,
  type SSECompleteEvent,
  type SSEErrorEvent,
  type SSEPRCommentEvent,
  type SSEEvent,
  type SSEConnectionStatus,
  // PR Comment types
  type PRComment,
  type PRCommentAuthor,
  type PRCommentsResponse,
  // File change types (for diff viewer)
  type FileChangeStatus,
  type FileChange,
  type ChangesSummary,
  type TaskChangesResponse,
  // API response types
  type ExecuteResponse,
  type ApproveResponse,
  type CancelResponse,
  type ExtendResponse,
  type FeedbackResponse,
  type RequestChangesResponse,
  type PRMergedResponse,
  type PRClosedResponse,
} from '@dash-agent/shared';
