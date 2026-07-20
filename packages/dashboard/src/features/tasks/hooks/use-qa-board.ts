'use client'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { tasksApi } from '@/lib/api-client'
import type { CreateQaRunInput, QaBoardItem } from '../types'
import { taskKeys } from './query-keys'

interface UseQABoardOptions {
  repositoryId?: string
}

export function useQABoard(options: UseQABoardOptions = {}) {
  const queryClient = useQueryClient()
  const filters = options.repositoryId ? { repository_id: options.repositoryId } : {}

  const board = useQuery({
    queryKey: taskKeys.qaBoard(filters),
    queryFn: () => tasksApi.getQABoard(filters),
    refetchInterval: (query) => {
      const data = query.state.data as QaBoardItem[] | undefined
      return data?.some((item) =>
        item.latest_run?.status === 'queued' || item.latest_run?.status === 'running'
      )
        ? 2500
        : false
    },
  })

  const startRun = useMutation({
    mutationFn: ({ taskId, input }: { taskId: string; input: CreateQaRunInput }) =>
      tasksApi.startQARun(taskId, input),
    onSuccess: (run) => {
      toast.success('QA run started')
      queryClient.invalidateQueries({ queryKey: taskKeys.qaBoards() })
      queryClient.invalidateQueries({ queryKey: taskKeys.qaRuns(run.task_id) })
      queryClient.invalidateQueries({ queryKey: taskKeys.detail(run.task_id) })
    },
    onError: (error: Error) => {
      toast.error(`Failed to start QA run: ${error.message}`)
    },
  })

  return {
    ...board,
    startRun,
  }
}
