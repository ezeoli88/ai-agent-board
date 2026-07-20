import type { Spec, SpecStatus } from '../types'

export type SpecColumnId =
  | 'idea'
  | 'clarifying'
  | 'specDraft'
  | 'specReview'
  | 'plan'
  | 'taskBreakdown'
  | 'ready'
  | 'implemented'
  | 'failed'

export interface SpecColumnConfig {
  id: SpecColumnId
  title: string
  statuses: SpecStatus[]
  color: string
  bgColor: string
  borderColor: string
}

export type SpecBoardColumns = Record<SpecColumnId, Spec[]>

export const SPEC_BOARD_COLUMNS: SpecColumnConfig[] = [
  {
    id: 'idea',
    title: 'Idea',
    statuses: ['idea'],
    color: 'text-gray-700 dark:text-gray-300',
    bgColor: 'bg-gray-100 dark:bg-gray-800/50',
    borderColor: 'border-gray-300 dark:border-gray-700',
  },
  {
    id: 'clarifying',
    title: 'Clarifying',
    statuses: ['clarifying'],
    color: 'text-sky-700 dark:text-sky-300',
    bgColor: 'bg-sky-50 dark:bg-sky-900/20',
    borderColor: 'border-sky-300 dark:border-sky-700',
  },
  {
    id: 'specDraft',
    title: 'Spec Draft',
    statuses: ['spec_draft'],
    color: 'text-blue-700 dark:text-blue-300',
    bgColor: 'bg-blue-50 dark:bg-blue-900/20',
    borderColor: 'border-blue-300 dark:border-blue-700',
  },
  {
    id: 'specReview',
    title: 'Spec Review',
    statuses: ['spec_review'],
    color: 'text-cyan-700 dark:text-cyan-300',
    bgColor: 'bg-cyan-50 dark:bg-cyan-900/20',
    borderColor: 'border-cyan-300 dark:border-cyan-700',
  },
  {
    id: 'plan',
    title: 'Plan',
    statuses: ['plan'],
    color: 'text-violet-700 dark:text-violet-300',
    bgColor: 'bg-violet-50 dark:bg-violet-900/20',
    borderColor: 'border-violet-300 dark:border-violet-700',
  },
  {
    id: 'taskBreakdown',
    title: 'Task Breakdown',
    statuses: ['task_breakdown'],
    color: 'text-amber-700 dark:text-amber-300',
    bgColor: 'bg-amber-50 dark:bg-amber-900/20',
    borderColor: 'border-amber-300 dark:border-amber-700',
  },
  {
    id: 'ready',
    title: 'Ready',
    statuses: ['ready_for_implementation'],
    color: 'text-green-700 dark:text-green-300',
    bgColor: 'bg-green-50 dark:bg-green-900/20',
    borderColor: 'border-green-300 dark:border-green-700',
  },
  {
    id: 'implemented',
    title: 'Implemented',
    statuses: ['implemented'],
    color: 'text-emerald-700 dark:text-emerald-300',
    bgColor: 'bg-emerald-50 dark:bg-emerald-900/20',
    borderColor: 'border-emerald-300 dark:border-emerald-700',
  },
  {
    id: 'failed',
    title: 'Failed',
    statuses: ['failed'],
    color: 'text-red-700 dark:text-red-300',
    bgColor: 'bg-red-50 dark:bg-red-900/20',
    borderColor: 'border-red-300 dark:border-red-700',
  },
]

export function groupSpecsByColumn(specs: Spec[]): SpecBoardColumns {
  const columns = Object.fromEntries(
    SPEC_BOARD_COLUMNS.map((column) => [column.id, []]),
  ) as SpecBoardColumns

  for (const spec of specs) {
    const column = SPEC_BOARD_COLUMNS.find((item) => item.statuses.includes(spec.status))
    columns[column?.id ?? 'idea'].push(spec)
  }

  return columns
}
