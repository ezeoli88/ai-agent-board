import type { TaskFilters } from './use-tasks'

export const taskKeys = {
  all: ['tasks'] as const,
  lists: () => [...taskKeys.all, 'list'] as const,
  list: (filters: TaskFilters) => [...taskKeys.lists(), filters] as const,
  details: () => [...taskKeys.all, 'detail'] as const,
  detail: (id: string) => [...taskKeys.details(), id] as const,
  changes: (id: string) => [...taskKeys.all, 'changes', id] as const,
  qaRuns: (id: string) => [...taskKeys.all, 'qa-runs', id] as const,
  qaRun: (runId: string) => [...taskKeys.all, 'qa-run', runId] as const,
  qaBoards: () => [...taskKeys.all, 'qa-board'] as const,
  qaBoard: (filters: { repository_id?: string }) => [...taskKeys.qaBoards(), filters] as const,
  prComments: (id: string) => [...taskKeys.all, 'pr-comments', id] as const,
}
