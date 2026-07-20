'use client'

import { useQuery } from '@tanstack/react-query'
import { tasksApi } from '@/lib/api-client'
import { taskKeys } from './query-keys'

export function useQARun(runId: string) {
  return useQuery({
    queryKey: taskKeys.qaRun(runId),
    queryFn: () => tasksApi.getQARun(runId),
    enabled: Boolean(runId),
    refetchInterval: (query) => {
      const run = query.state.data
      return run?.status === 'queued' || run?.status === 'running' ? 2500 : false
    },
  })
}
