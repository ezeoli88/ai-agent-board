'use client'

import { useQuery } from '@tanstack/react-query'
import { specKeys } from './query-keys'
import { specsApi } from '@/lib/api-client'

export function useSpec(id: string | null) {
  return useQuery({
    queryKey: specKeys.detail(id!),
    queryFn: () => specsApi.getById(id!),
    enabled: !!id,
  })
}
