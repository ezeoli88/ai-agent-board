import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { tasksApi } from '@/lib/api-client'
import type { CreateQaRunInput, QaRun } from '../types'
import { taskKeys } from './query-keys'

export function useTaskQARuns(taskId: string) {
  const queryClient = useQueryClient()

  const runs = useQuery({
    queryKey: taskKeys.qaRuns(taskId),
    queryFn: () => tasksApi.getQARuns(taskId),
    enabled: Boolean(taskId),
    refetchInterval: (query) => {
      const data = query.state.data as QaRun[] | undefined
      return data?.some((run) => run.status === 'queued' || run.status === 'running') ? 2500 : false
    },
  })

  const startRun = useMutation({
    mutationFn: (input: CreateQaRunInput) => tasksApi.startQARun(taskId, input),
    onSuccess: () => {
      toast.success('QA run started')
      queryClient.invalidateQueries({ queryKey: taskKeys.qaRuns(taskId) })
      queryClient.invalidateQueries({ queryKey: taskKeys.detail(taskId) })
    },
    onError: (error: Error) => {
      toast.error(`Failed to start QA run: ${error.message}`)
    },
  })

  return {
    ...runs,
    startRun,
  }
}
