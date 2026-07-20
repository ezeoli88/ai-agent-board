// Task types
export {
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
} from './task.types.js';

// Spec types
export {
  SPEC_STATUS_LABELS,
  SPEC_STATUS_COLORS,
  isSpecTerminalStatus,
  getSpecAvailableActions,
} from './spec.types.js';

// SSE types
export type {
  LogLevel,
  LogEntry,
  SSEEventType,
  SSELogEvent,
  SSEStatusEvent,
  SSETimeoutWarningEvent,
  SSEAwaitingReviewEvent,
  SSECompleteEvent,
  SSEErrorEvent,
  SSEPRCommentEvent,
  SSEEvent,
  SSEConnectionStatus,
} from './sse.types.js';

// API types
export {
  ERROR_CODES,
  type ErrorCode,
  type AppError,
  type ExecuteResponse,
  type ApproveResponse,
  type CancelResponse,
  type ExtendResponse,
  type FeedbackResponse,
  type RequestChangesResponse,
  type PRMergedResponse,
  type PRClosedResponse,
  type PaginationParams,
  type PaginatedResponse,
  type TaskFilters,
  // Re-exported from schemas
  type FileChangeStatus,
  type FileChange,
  type ChangesSummary,
  type TaskChangesResponse,
  type ApiError,
} from './api.types.js';

// Setup types
export {
  AI_PROVIDER_INFO,
  SETUP_ERROR_CODES,
  type AIProviderInfo,
  type SetupStep,
  type StepStatus,
  type SetupStepInfo,
  type ValidationState,
  type GitHubConnectionState,
  type SetupErrorCode,
  type SetupError,
} from './setup.types.js';
