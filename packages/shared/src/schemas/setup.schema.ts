import { z } from 'zod';

// ============================================================================
// AI Provider Schemas
// ============================================================================

/**
 * Supported AI providers
 */
export const AIProviderSchema = z.enum(['claude', 'openai', 'openrouter', 'minimax']);
export type AIProvider = z.infer<typeof AIProviderSchema>;

/**
 * AI providers list for UI iteration
 */
export const AI_PROVIDERS = ['claude', 'openai', 'openrouter', 'minimax'] as const;

// ============================================================================
// OpenRouter Schemas
// ============================================================================

/**
 * OpenRouter model pricing information
 */
export const OpenRouterPricingSchema = z.object({
  prompt: z.string(), // Price per token as string (e.g., "0" for free)
  completion: z.string(),
});
export type OpenRouterPricing = z.infer<typeof OpenRouterPricingSchema>;

/**
 * OpenRouter model information
 */
export const OpenRouterModelSchema = z.object({
  id: z.string(),
  name: z.string(),
  pricing: OpenRouterPricingSchema.optional(),
});
export type OpenRouterModel = z.infer<typeof OpenRouterModelSchema>;

/**
 * Request to validate an OpenRouter API key
 */
export const ValidateOpenRouterKeyRequestSchema = z.object({
  apiKey: z.string().min(1, 'API key is required'),
});
export type ValidateOpenRouterKeyRequest = z.infer<typeof ValidateOpenRouterKeyRequestSchema>;

/**
 * Response from validating an OpenRouter API key
 */
export const ValidateOpenRouterKeyResponseSchema = z.object({
  valid: z.boolean(),
  models: z.array(OpenRouterModelSchema).optional(),
  freeModels: z.array(OpenRouterModelSchema).optional(),
  error: z.string().optional(),
});
export type ValidateOpenRouterKeyResponse = z.infer<typeof ValidateOpenRouterKeyResponseSchema>;

/**
 * Request to validate an AI provider API key
 */
export const ValidateAIKeyRequestSchema = z.object({
  provider: AIProviderSchema,
  apiKey: z.string().min(1, 'API key is required'),
});
export type ValidateAIKeyRequest = z.infer<typeof ValidateAIKeyRequestSchema>;

/**
 * Response from validating an AI provider API key
 */
export const ValidateAIKeyResponseSchema = z.object({
  valid: z.boolean(),
  provider: AIProviderSchema,
  modelInfo: z.object({
    name: z.string(),
    description: z.string(),
  }).optional(),
  error: z.string().optional(),
});
export type ValidateAIKeyResponse = z.infer<typeof ValidateAIKeyResponseSchema>;

// ============================================================================
// GitHub OAuth Schemas (used by server - frontend uses PAT flow only)
// ============================================================================

/**
 * Response from getting GitHub auth URL
 */
export const GitHubAuthUrlResponseSchema = z.object({
  authUrl: z.string().url(),
  state: z.string(),
});
export type GitHubAuthUrlResponse = z.infer<typeof GitHubAuthUrlResponseSchema>;

/**
 * Request for GitHub OAuth callback
 */
export const GitHubCallbackRequestSchema = z.object({
  code: z.string().min(1, 'Authorization code is required'),
  state: z.string().min(1, 'State is required'),
});
export type GitHubCallbackRequest = z.infer<typeof GitHubCallbackRequestSchema>;

/**
 * Response from GitHub OAuth callback
 */
export const GitHubCallbackResponseSchema = z.object({
  success: z.boolean(),
  username: z.string().optional(),
  avatarUrl: z.string().url().optional(),
  error: z.string().optional(),
});
export type GitHubCallbackResponse = z.infer<typeof GitHubCallbackResponseSchema>;

// ============================================================================
// Setup Status Schemas
// ============================================================================

/**
 * Complete setup configuration status
 */
export const SetupStatusSchema = z.object({
  aiProvider: AIProviderSchema.nullable(),
  aiConnected: z.boolean(),
  githubConnected: z.boolean(),
  githubUsername: z.string().nullable(),
  githubAvatarUrl: z.string().url().nullable(),
  isComplete: z.boolean(),
});
export type SetupStatus = z.infer<typeof SetupStatusSchema>;

/**
 * Response for disconnecting a provider
 */
export const DisconnectResponseSchema = z.object({
  success: z.boolean(),
  message: z.string(),
});
export type DisconnectResponse = z.infer<typeof DisconnectResponseSchema>;

// ============================================================================
// Chrome MCP Config
// ============================================================================

export const ChromeMcpConfigSchema = z.object({
  enabled: z.boolean(),
  url: z.string().url().nullable(),
});
export type ChromeMcpConfig = z.infer<typeof ChromeMcpConfigSchema>;

export const UpdateChromeMcpConfigSchema = z.object({
  enabled: z.boolean(),
  url: z.string().nullable().optional(),
});
export type UpdateChromeMcpConfigInput = z.infer<typeof UpdateChromeMcpConfigSchema>;

export const UpdateChromeMcpConfigResponseSchema = z.object({
  success: z.boolean(),
  config: ChromeMcpConfigSchema,
});
export type UpdateChromeMcpConfigResponse = z.infer<typeof UpdateChromeMcpConfigResponseSchema>;

// ============================================================================
// localStorage Config (frontend only, not validated by server)
// ============================================================================

/**
 * Setup configuration stored in localStorage
 * Note: API keys should be encrypted before storage (handled by frontend)
 */
export const SetupConfigSchema = z.object({
  aiProvider: AIProviderSchema.nullable(),
  aiApiKey: z.string().nullable(),
  openRouterModel: z.string().nullable(), // Selected OpenRouter model ID
  githubConnected: z.boolean(),
  githubUsername: z.string().nullable(),
  githubAvatarUrl: z.string().nullable(),
  githubToken: z.string().nullable(),
});
export type SetupConfig = z.infer<typeof SetupConfigSchema>;

/**
 * Default setup configuration
 */
export const DEFAULT_SETUP_CONFIG: SetupConfig = {
  aiProvider: null,
  aiApiKey: null,
  openRouterModel: null,
  githubConnected: false,
  githubUsername: null,
  githubAvatarUrl: null,
  githubToken: null,
};
