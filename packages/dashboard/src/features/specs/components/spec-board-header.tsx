'use client'

import { Plus, RefreshCw } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { useRolePermissions } from '@/features/settings'
import { useRepoStore } from '@/features/repos'
import { specKeys } from '../hooks/query-keys'
import { useSpecUIStore } from '../stores/spec-ui-store'

export function SpecBoardHeader() {
  const queryClient = useQueryClient()
  const selectedRepo = useRepoStore((state) => state.selectedRepo)
  const openCreateModal = useSpecUIStore((state) => state.openCreateModal)
  const { canManageSpecs } = useRolePermissions()

  return (
    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Specs</h1>
        <p className="text-sm text-muted-foreground">
          {selectedRepo?.name ?? 'Selected repository'} · spec-driven demo workflow
        </p>
      </div>
      <div className="flex gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={() => queryClient.invalidateQueries({ queryKey: specKeys.all })}
        >
          <RefreshCw className="h-4 w-4" />
          Refresh
        </Button>
        {canManageSpecs && (
          <Button size="sm" onClick={openCreateModal}>
            <Plus className="h-4 w-4" />
            New Spec
          </Button>
        )}
      </div>
    </div>
  )
}
