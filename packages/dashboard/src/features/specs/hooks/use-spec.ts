'use client'

import { useQuery } from '@tanstack/react-query'
import { specsApi } from '@/lib/api-client'
import { specKeys } from './query-keys'

export function useSpec(specId: string) {
  return useQuery({
    queryKey: specKeys.detail(specId),
    queryFn: () => specsApi.getById(specId),
    enabled: !!specId,
  })
}
