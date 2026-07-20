'use client'

import { useQuery } from '@tanstack/react-query'
import { specsApi } from '@/lib/api-client'
import { specKeys } from './query-keys'

export function useSpecs(filters?: { repository_id?: string }) {
  return useQuery({
    queryKey: specKeys.list(filters),
    queryFn: () => specsApi.getAll(filters),
  })
}
