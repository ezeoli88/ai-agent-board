'use client'

import { useQuery } from '@tanstack/react-query'
import { specKeys } from './query-keys'
import { specsApi } from '@/lib/api-client'

export function useSpecs(filters: { repository_id?: string } = {}) {
  return useQuery({
    queryKey: specKeys.list(filters),
    queryFn: () => specsApi.getAll(filters),
    staleTime: 30_000,
    refetchInterval: (query) => {
      const specs = query.state.data
      if (specs?.some(s => s.status === 'refining')) return 3000
      return false
    },
  })
}
