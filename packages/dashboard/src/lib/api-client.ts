import type {
  Task,
  CreateTaskInput,
  UpdateTaskInput,
  CreateQaRunInput,
  QaRun,
  QaBoardItem,
  TaskChangesResponse,
  RequestChangesResponse,
  PRMergedResponse,
  PRClosedResponse,
  PRCommentsResponse,
} from "@/features/tasks/types";
import type {
  Spec,
  CreateSpecInput,
  UpdateSpecInput,
  AnswerClarificationsInput,
  CreateTasksFromSpecResponse,
} from "@/features/specs/types";
import type { ActionResponse, CleanupWorktreeResponse } from "@/types/api";
import { getAuthToken } from "./auth";

// API error response from backend (matches shared ApiError schema)
interface ApiErrorResponse {
  error: string;
  message?: string;
  details?: { field: string; message: string }[];
  code?: string;
}

// Base URL: use env var in dev, otherwise same origin (production/binary mode)
const API_BASE_URL =
  import.meta.env.VITE_API_BASE_URL || window.location.origin;

// Custom error class for API errors
export class ApiClientError extends Error {
  public readonly statusCode: number;
  public readonly code?: string;
  public readonly details?: Record<string, string[]>;

  constructor(
    message: string,
    statusCode: number,
    code?: string,
    details?: Record<string, string[]>,
  ) {
    super(message);
    this.name = "ApiClientError";
    this.statusCode = statusCode;
    this.code = code;
    this.details = details;
  }
}

// Request options type
interface RequestOptions extends Omit<RequestInit, "body"> {
  body?: unknown;
  params?: Record<string, string | number | boolean | undefined>;
}

function normalizeStringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

function normalizeSpec(spec: Spec): Spec {
  const raw = spec as Spec & Record<string, unknown>;
  return {
    ...spec,
    clarification_questions: normalizeStringArray(raw.clarification_questions),
    clarification_answers: normalizeStringArray(raw.clarification_answers),
    task_ids: normalizeStringArray(raw.task_ids),
  };
}

// Build URL with query parameters
function buildUrl(
  endpoint: string,
  params?: Record<string, string | number | boolean | undefined>,
): string {
  const url = new URL(`/api${endpoint}`, API_BASE_URL);

  if (params) {
    Object.entries(params).forEach(([key, value]) => {
      if (value !== undefined) {
        url.searchParams.append(key, String(value));
      }
    });
  }

  return url.toString();
}

/**
 * Gets AI configuration from localStorage
 */
function getAIConfigHeaders(): Record<string, string> {
  if (typeof window === "undefined") return {};

  try {
    const setupConfig = localStorage.getItem("dash-agent-setup-config");
    if (setupConfig) {
      const config = JSON.parse(setupConfig);
      if (config.aiProvider && config.aiApiKey) {
        return {
          "X-AI-Provider": config.aiProvider,
          "X-AI-API-Key": config.aiApiKey,
        };
      }
    }
  } catch (e) {
    console.error("Failed to get AI config from localStorage:", e);
  }

  return {};
}

// Generic fetch wrapper
async function request<T>(
  endpoint: string,
  options: RequestOptions = {},
): Promise<T> {
  const { body, params, headers: customHeaders, ...restOptions } = options;

  const url = buildUrl(endpoint, params);

  const authToken = getAuthToken();
  const headers: HeadersInit = {
    "Content-Type": "application/json",
    ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
    ...customHeaders,
  };

  const config: RequestInit = {
    ...restOptions,
    headers,
    cache: "no-store",
    body: body ? JSON.stringify(body) : undefined,
  };

  try {
    const response = await fetch(url, config);

    // Detect server restarts via startup ID header
    const serverId = response.headers.get("X-Server-ID");
    if (serverId) {
      const storedId = localStorage.getItem("agent-board-server-id");
      if (storedId && storedId !== serverId) {
        // Server restarted — store new ID and notify the app
        localStorage.setItem("agent-board-server-id", serverId);
        window.dispatchEvent(new Event("server-restart"));
      } else if (!storedId) {
        // First request — just store the ID
        localStorage.setItem("agent-board-server-id", serverId);
      }
    }

    // Handle non-OK responses
    if (!response.ok) {
      let errorData: ApiErrorResponse | null = null;

      try {
        errorData = await response.json();
      } catch {
        // Response is not JSON
      }

      // Convert array-based details to Record format
      let details: Record<string, string[]> | undefined;
      if (errorData?.details) {
        details = {};
        for (const detail of errorData.details) {
          if (!details[detail.field]) {
            details[detail.field] = [];
          }
          details[detail.field].push(detail.message);
        }
      }

      throw new ApiClientError(
        errorData?.message ||
          errorData?.error ||
          `HTTP Error: ${response.status} ${response.statusText}`,
        response.status,
        errorData?.code,
        details,
      );
    }

    // Handle 204 No Content
    if (response.status === 204) {
      return undefined as T;
    }

    return response.json();
  } catch (error) {
    if (error instanceof ApiClientError) {
      throw error;
    }

    // Network or other errors
    throw new ApiClientError(
      error instanceof Error ? error.message : "An unknown error occurred",
      0,
    );
  }
}

// API client with typed methods
export const apiClient = {
  // Generic methods
  get: <T>(endpoint: string, options?: RequestOptions) =>
    request<T>(endpoint, { ...options, method: "GET" }),

  post: <T>(endpoint: string, body?: unknown, options?: RequestOptions) =>
    request<T>(endpoint, { ...options, method: "POST", body }),

  put: <T>(endpoint: string, body?: unknown, options?: RequestOptions) =>
    request<T>(endpoint, { ...options, method: "PUT", body }),

  patch: <T>(endpoint: string, body?: unknown, options?: RequestOptions) =>
    request<T>(endpoint, { ...options, method: "PATCH", body }),

  delete: <T>(endpoint: string, options?: RequestOptions) =>
    request<T>(endpoint, { ...options, method: "DELETE" }),

  // Task-specific endpoints
  tasks: {
    getAll: (params?: {
      status?: string;
      search?: string;
      repository_id?: string;
    }) => apiClient.get<Task[]>("/tasks", { params }),

    getById: (id: string) => apiClient.get<Task>(`/tasks/${id}`),

    create: (data: CreateTaskInput) => apiClient.post<Task>("/tasks", data),

    update: (id: string, data: UpdateTaskInput) =>
      apiClient.patch<Task>(`/tasks/${id}`, data),

    delete: (id: string) => apiClient.delete<void>(`/tasks/${id}`),

    // Task actions
    start: (id: string) => apiClient.post<Task>(`/tasks/${id}/start`),

    cancel: (id: string) => apiClient.post<Task>(`/tasks/${id}/cancel`),

    retry: (id: string) => apiClient.post<Task>(`/tasks/${id}/retry`),

    // Get task logs
    getLogs: (id: string) => apiClient.get<string[]>(`/tasks/${id}/logs`),
  },
};

// Standalone tasksApi for use with TanStack Query hooks
export const tasksApi = {
  getAll: async (filters?: {
    status?: string[];
    search?: string;
    repository_id?: string;
  }) => {
    const params: Record<string, string | undefined> = {};

    if (filters?.status && filters.status.length > 0) {
      params.status = filters.status.join(",");
    }
    if (filters?.search) {
      params.search = filters.search;
    }
    if (filters?.repository_id) {
      params.repository_id = filters.repository_id;
    }

    return apiClient.get<Task[]>("/tasks", { params });
  },

  getById: (id: string) => apiClient.get<Task>(`/tasks/${id}`),

  create: (data: CreateTaskInput) => apiClient.post<Task>("/tasks", data),

  update: (id: string, data: UpdateTaskInput) =>
    apiClient.patch<Task>(`/tasks/${id}`, data),

  delete: (id: string) => apiClient.delete<void>(`/tasks/${id}`),

  getLogs: (id: string) => apiClient.get<string[]>(`/tasks/${id}/logs`),

  // ==========================================================================
  // Task Actions
  // ==========================================================================

  /** Start a draft task (status: draft -> running) */
  start: (id: string) =>
    apiClient.post<ActionResponse>(`/tasks/${id}/start`),

  /** Approve plan and start implementation (status: plan_review -> coding) */
  approvePlan: (id: string) =>
    apiClient.post<ActionResponse>(`/tasks/${id}/approve-plan`),

  /** Execute task (start Dev Agent or legacy agent) */
  execute: (id: string) =>
    apiClient.post<ActionResponse>(`/tasks/${id}/execute`),

  /** Approve and create PR (status: awaiting_review/review -> pr_created) */
  approve: (id: string) =>
    apiClient.post<ActionResponse>(`/tasks/${id}/approve`),

  /** Cancel running agent */
  cancel: (id: string) => apiClient.post<ActionResponse>(`/tasks/${id}/cancel`),

  /** Extend agent timeout */
  extend: (id: string) => apiClient.post<ActionResponse>(`/tasks/${id}/extend`),

  /** Send feedback to running agent */
  feedback: (id: string, message: string) =>
    apiClient.post<ActionResponse>(`/tasks/${id}/feedback`, { message }),

  // PR-related action endpoints
  requestChanges: (id: string, feedback: string) =>
    apiClient.post<RequestChangesResponse>(`/tasks/${id}/request-changes`, {
      feedback,
    }),

  markPRMerged: (id: string) =>
    apiClient.post<PRMergedResponse>(`/tasks/${id}/pr-merged`),

  markPRClosed: (id: string) =>
    apiClient.post<PRClosedResponse>(`/tasks/${id}/pr-closed`),

  getChanges: (id: string): Promise<TaskChangesResponse> =>
    apiClient.get<TaskChangesResponse>(`/tasks/${id}/changes`),

  getQARuns: (id: string): Promise<QaRun[]> =>
    apiClient.get<QaRun[]>(`/tasks/${id}/qa-runs`),

  getQARun: (runId: string): Promise<QaRun> =>
    apiClient.get<QaRun>(`/tasks/qa-runs/${runId}`),

  getQABoard: (filters?: { repository_id?: string }): Promise<QaBoardItem[]> =>
    apiClient.get<QaBoardItem[]>("/tasks/qa-board/items", {
      params: { repository_id: filters?.repository_id },
    }),

  startQARun: (id: string, data: CreateQaRunInput): Promise<QaRun> =>
    apiClient.post<QaRun>(`/tasks/${id}/qa-runs`, data),

  cleanupWorktree: (id: string): Promise<CleanupWorktreeResponse> =>
    apiClient.post<CleanupWorktreeResponse>(`/tasks/${id}/cleanup-worktree`),

  // PR comments
  getPRComments: (id: string): Promise<PRCommentsResponse> =>
    apiClient.get<PRCommentsResponse>(`/tasks/${id}/pr-comments`),
};

export const specsApi = {
  getAll: (filters?: { repository_id?: string }) => {
    const params: Record<string, string | undefined> = {};
    if (filters?.repository_id) {
      params.repository_id = filters.repository_id;
    }
    return apiClient.get<Spec[]>("/specs", { params }).then((specs) => specs.map(normalizeSpec));
  },

  getById: (id: string) => apiClient.get<Spec>(`/specs/${id}`).then(normalizeSpec),

  create: (data: CreateSpecInput) => apiClient.post<Spec>("/specs", data).then(normalizeSpec),

  update: (id: string, data: UpdateSpecInput) =>
    apiClient.patch<Spec>(`/specs/${id}`, data).then(normalizeSpec),

  delete: (id: string) => apiClient.delete<void>(`/specs/${id}`),

  generateSpec: (id: string) =>
    apiClient.post<Spec>(`/specs/${id}/generate-spec`).then(normalizeSpec),

  answerClarifications: (id: string, data: AnswerClarificationsInput) =>
    apiClient.post<Spec>(`/specs/${id}/answer-clarifications`, data).then(normalizeSpec),

  getAgentStatus: (id: string) =>
    apiClient.get<{ running: boolean }>(`/specs/${id}/agent-status`),

  sendFeedback: (id: string, message: string) =>
    apiClient.post<{ status: string }>(`/specs/${id}/feedback`, { message }),

  approveSpec: (id: string, content?: string) =>
    apiClient.post<Spec>(`/specs/${id}/approve-spec`, { content }).then(normalizeSpec),

  generatePlan: (id: string) =>
    apiClient.post<Spec>(`/specs/${id}/generate-plan`).then(normalizeSpec),

  approvePlan: (id: string, content?: string) =>
    apiClient.post<Spec>(`/specs/${id}/approve-plan`, { content }).then(normalizeSpec),

  generateTasks: (id: string) =>
    apiClient.post<Spec>(`/specs/${id}/generate-tasks`).then(normalizeSpec),

  approveTasks: (id: string, content?: string) =>
    apiClient.post<Spec>(`/specs/${id}/approve-tasks`, { content }).then(normalizeSpec),

  createTasks: (id: string) =>
    apiClient
      .post<CreateTasksFromSpecResponse>(`/specs/${id}/create-tasks`)
      .then((response) => ({ ...response, spec: normalizeSpec(response.spec) })),
};

// Export types for consumers
export type { RequestOptions };

// Export ActionResponse for external use
export type { ActionResponse, CleanupWorktreeResponse };
