// Re-export all setup types from shared package
export type {
  AIProvider,
  ValidateAIKeyRequest,
  ValidateAIKeyResponse,
  SetupStatus,
  DisconnectResponse,
  ChromeMcpConfig,
  UpdateChromeMcpConfigInput,
  UpdateChromeMcpConfigResponse,
  SetupConfig,
  // OpenRouter types
  OpenRouterModel,
  OpenRouterPricing,
  ValidateOpenRouterKeyRequest,
  ValidateOpenRouterKeyResponse,
  // Secrets types
  GitHubConnectionMethod,
  SaveAISecretRequest,
  SaveAISecretResponse,
  AISecretStatus,
  SaveGitHubSecretRequest,
  SaveGitHubSecretResponse,
  GitHubSecretStatus,
  ValidateGitHubPATRequest,
  ValidateGitHubPATResponse,
  // GitLab types
  ValidateGitLabPATRequest,
  ValidateGitLabPATResponse,
  SaveGitLabSecretRequest,
  SaveGitLabSecretResponse,
  GitLabSecretStatus,
  AllSecretsStatus,
  DeleteSecretResponse,
} from '@dash-agent/shared'

export type {
  AIProviderInfo,
  SetupStep,
  StepStatus,
  SetupStepInfo,
  ValidationState,
  GitHubConnectionState,
  SetupErrorCode,
  SetupError,
} from '@dash-agent/shared'

export {
  AI_PROVIDERS,
  DEFAULT_SETUP_CONFIG,
  AI_PROVIDER_INFO,
  SETUP_ERROR_CODES,
} from '@dash-agent/shared'
