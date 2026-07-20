'use client'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  ChromeMcpConfig,
  UpdateChromeMcpConfigInput,
  UpdateChromeMcpConfigResponse,
} from '@dash-agent/shared'
import { apiClient } from '@/lib/api-client'

const chromeMcpConfigKey = ['setup', 'chrome-mcp-config'] as const

export function useChromeMcpConfig() {
  return useQuery({
    queryKey: chromeMcpConfigKey,
    queryFn: () => apiClient.get<ChromeMcpConfig>('/setup/chrome-mcp-config'),
  })
}

export function useUpdateChromeMcpConfig() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (input: UpdateChromeMcpConfigInput) =>
      apiClient.patch<UpdateChromeMcpConfigResponse>('/setup/chrome-mcp-config', input),
    onSuccess: (response) => {
      queryClient.setQueryData(chromeMcpConfigKey, response.config)
      queryClient.invalidateQueries({ queryKey: chromeMcpConfigKey })
    },
  })
}
