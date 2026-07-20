/**
 * @agent-board/shared
 *
 * Shared types and schemas for Agent Board frontend and backend.
 * This package serves as the single source of truth for all TypeScript types
 * and Zod validation schemas used across the application.
 */

// ============================================================================
// Schemas (Zod validation + inferred types)
// ============================================================================

// Task schemas
export {
  TASK_STATUSES,
  QA_RUN_STATUSES,
  TaskStatusSchema,
  QaRunStatusSchema,
  QaRunSchema,
  CreateQaRunSchema,
  TaskSchema,
  QaBoardItemSchema,
  CreateTaskSchema,
  UpdateTaskSchema,
  // Two-agent workflow schemas
  GenerateSpecRequestSchema,
  GenerateSpecResponseSchema,
  UpdateSpecRequestSchema,
  ApproveSpecRequestSchema,
  ApproveSpecResponseSchema,
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
} from './schemas/task.schema.js';

// Spec schemas
export {
  SPEC_STATUSES,
  SpecStatusSchema,
  SpecSchema,
  CreateSpecSchema,
  UpdateSpecSchema,
  ApproveSpecStepSchema,
  AnswerClarificationsSchema,
  CreateTasksFromSpecResponseSchema,
  type SpecStatus,
  type Spec,
  type CreateSpecInput,
  type UpdateSpecInput,
  type ApproveSpecStepInput,
  type AnswerClarificationsInput,
  type CreateTasksFromSpecResponse,
} from './schemas/spec.schema.js';

// Feedback schemas
export {
  FeedbackSchema,
  RequestChangesSchema,
  type FeedbackInput,
  type RequestChangesInput,
} from './schemas/feedback.schema.js';

// API schemas
export {
  FileChangeStatusSchema,
  FileChangeSchema,
  ChangesSummarySchema,
  TaskChangesResponseSchema,
  ApiErrorDetailSchema,
  ApiErrorSchema,
  // Chat event schemas
  ChatMessageEventSchema,
  ToolActivityEventSchema,
  type ChatMessageEvent,
  type ToolActivityEvent,
} from './schemas/api.schema.js';

// PR Comment schemas
export {
  PRCommentAuthorSchema,
  PRCommentSchema,
  PRCommentsResponseSchema,
  type PRCommentAuthor,
  type PRComment,
  type PRCommentsResponse,
} from './schemas/pr-comment.schema.js';

// Setup schemas
export {
  AIProviderSchema,
  AI_PROVIDERS,
  ValidateAIKeyRequestSchema,
  ValidateAIKeyResponseSchema,
  GitHubAuthUrlResponseSchema,
  GitHubCallbackRequestSchema,
  GitHubCallbackResponseSchema,
  SetupStatusSchema,
  DisconnectResponseSchema,
  ChromeMcpConfigSchema,
  UpdateChromeMcpConfigSchema,
  UpdateChromeMcpConfigResponseSchema,
  SetupConfigSchema,
  DEFAULT_SETUP_CONFIG,
  // OpenRouter schemas
  OpenRouterPricingSchema,
  OpenRouterModelSchema,
  ValidateOpenRouterKeyRequestSchema,
  ValidateOpenRouterKeyResponseSchema,
  type AIProvider,
  type ValidateAIKeyRequest,
  type ValidateAIKeyResponse,
  type GitHubAuthUrlResponse,
  type GitHubCallbackRequest,
  type GitHubCallbackResponse,
  type SetupStatus,
  type DisconnectResponse,
  type ChromeMcpConfig,
  type UpdateChromeMcpConfigInput,
  type UpdateChromeMcpConfigResponse,
  type SetupConfig,
  // OpenRouter types
  type OpenRouterPricing,
  type OpenRouterModel,
  type ValidateOpenRouterKeyRequest,
  type ValidateOpenRouterKeyResponse,
} from './schemas/setup.schema.js';

// Repository schemas
export {
  DetectedStackSchema,
  DEFAULT_DETECTED_STACK,
  LearnedPatternSchema,
  RepositorySchema,
  CreateRepositorySchema,
  UpdateRepositorySchema,
  GitHubRepositorySchema,
  GitHubReposResponseSchema,
  StackDetectionResponseSchema,
  ClearPatternsResponseSchema,
  AddPatternRequestSchema,
  AddPatternResponseSchema,
  DeletePatternResponseSchema,
  type DetectedStack,
  type LearnedPattern,
  type Repository,
  type CreateRepositoryInput,
  type UpdateRepositoryInput,
  type GitHubRepository,
  type GitHubReposResponse,
  type StackDetectionResponse,
  type ClearPatternsResponse,
  type AddPatternRequest,
  type AddPatternResponse,
  type DeletePatternResponse,
  LocalRepositorySchema,
  LocalReposResponseSchema,
  type LocalRepository,
  type LocalReposResponse,
} from './schemas/repository.schema.js';

// ============================================================================
// Types (pure TypeScript types and utilities)
// ============================================================================

// Task type utilities
export {
  TASK_STATUS_LABELS,
  TASK_STATUS_COLORS,
  getAvailableActionsForStatus,
  isTerminalStatus,
  isActiveStatus,
  requiresUserAction,
  // Two-agent workflow utilities
  isSpecPhase,
  isCodingPhase,
  mapLegacyStatus,
  getPhaseForStatus,
} from './types/task.types.js';

// Spec type utilities
export {
  SPEC_STATUS_LABELS,
  SPEC_STATUS_COLORS,
  isSpecTerminalStatus,
  getSpecAvailableActions,
} from './types/spec.types.js';

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
} from './types/sse.types.js';

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
  type FileChangeStatus,
  type FileChange,
  type ChangesSummary,
  type TaskChangesResponse,
  type ApiError,
} from './types/api.types.js';

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
} from './types/setup.types.js';

// Agent schemas
export {
  AgentTypeSchema,
  AGENT_TYPES,
  AgentModelSchema,
  DetectedAgentSchema,
  DetectedAgentsResponseSchema,
  type AgentType,
  type AgentModel,
  type DetectedAgent,
  type DetectedAgentsResponse,
} from './schemas/agent.schema.js';

// Secrets schemas
export {
  GitHubConnectionMethodSchema,
  SaveAISecretRequestSchema,
  SaveAISecretResponseSchema,
  AISecretStatusSchema,
  SaveGitHubSecretRequestSchema,
  SaveGitHubSecretResponseSchema,
  GitHubSecretStatusSchema,
  ValidateGitHubPATRequestSchema,
  ValidateGitHubPATResponseSchema,
  AllSecretsStatusSchema,
  DeleteSecretResponseSchema,
  // GitLab schemas
  ValidateGitLabPATRequestSchema,
  ValidateGitLabPATResponseSchema,
  SaveGitLabSecretRequestSchema,
  SaveGitLabSecretResponseSchema,
  GitLabSecretStatusSchema,
  type GitHubConnectionMethod,
  type SaveAISecretRequest,
  type SaveAISecretResponse,
  type AISecretStatus,
  type SaveGitHubSecretRequest,
  type SaveGitHubSecretResponse,
  type GitHubSecretStatus,
  type ValidateGitHubPATRequest,
  type ValidateGitHubPATResponse,
  type AllSecretsStatus,
  type DeleteSecretResponse,
  // GitLab types
  type ValidateGitLabPATRequest,
  type ValidateGitLabPATResponse,
  type SaveGitLabSecretRequest,
  type SaveGitLabSecretResponse,
  type GitLabSecretStatus,
} from './schemas/secrets.schema.js';
