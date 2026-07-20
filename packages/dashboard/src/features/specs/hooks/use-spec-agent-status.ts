'use client'

import { useQuery } from '@tanstack/react-query'
import { specsApi } from '@/lib/api-client'
import { specKeys } from './query-keys'

export function useSpecAgentStatus(specId: string, enabled = true) {
  return useQuery({
    queryKey: specKeys.agentStatus(specId),
    queryFn: () => specsApi.getAgentStatus(specId),
    enabled: enabled && !!specId,
    refetchInterval: 1000,
  })
}
