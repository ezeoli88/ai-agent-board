'use client'

import { AlertCircle, RefreshCw } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { useRepoStore } from '@/features/repos'
import { useSpecs } from '../hooks/use-specs'
import { specKeys } from '../hooks/query-keys'
import { groupSpecsByColumn, SPEC_BOARD_COLUMNS } from './spec-board-types'
import { SpecBoardHeader } from './spec-board-header'
import { SpecColumn, SpecColumnSkeleton } from './spec-column'

export function SpecBoardView() {
  const queryClient = useQueryClient()
  const selectedRepoId = useRepoStore((state) => state.selectedRepoId)
  const { data: specs, isLoading, isError, error } = useSpecs(
    selectedRepoId ? { repository_id: selectedRepoId } : undefined,
  )

  const columns = groupSpecsByColumn(specs ?? [])

  if (isError) {
    return (
      <div className="flex flex-col gap-6">
        <SpecBoardHeader />
        <div className="flex flex-col items-center justify-center gap-4 rounded-lg border border-destructive/50 bg-destructive/10 p-8">
          <AlertCircle className="h-8 w-8 text-destructive" />
          <div className="text-center">
            <h3 className="font-semibold text-destructive">Failed to load specs</h3>
            <p className="text-sm text-muted-foreground">
              {error?.message || 'An unexpected error occurred'}
            </p>
          </div>
          <Button variant="outline" onClick={() => queryClient.invalidateQueries({ queryKey: specKeys.all })}>
            <RefreshCw className="h-4 w-4" />
            Retry
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      <SpecBoardHeader />

      <div className="flex gap-4 overflow-x-auto pb-4 -mx-4 px-4 md:-mx-6 md:px-6">
        {isLoading
          ? SPEC_BOARD_COLUMNS.map((column) => <SpecColumnSkeleton key={column.id} />)
          : SPEC_BOARD_COLUMNS.map((column) => (
              <SpecColumn key={column.id} config={column} specs={columns[column.id]} />
            ))}
      </div>
    </div>
  )
}
