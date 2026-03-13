'use client'

import { useMemo } from 'react'
import { AlertCircle, FileText, Plus, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/shared/empty-state'
import { useSpecs } from '../hooks/use-specs'
import { useSpecUIStore } from '../stores/spec-ui-store'
import { SpecListItem } from './spec-list-item'
import { useRepoStore } from '@/features/repos/stores/repo-store'
import type { SpecStatus } from '../types'
import { SPEC_STATUS_LABELS } from '../types'
import { cn } from '@/lib/utils'

const FILTER_STATUSES: SpecStatus[] = ['draft', 'refining', 'approved', 'failed', 'canceled']

export function SpecList() {
  const { selectedRepoId } = useRepoStore()
  const { statusFilter, setStatusFilter, drawerSpecId, openCreateModal } = useSpecUIStore()

  const filters = useMemo(
    () => ({
      repository_id: selectedRepoId ?? undefined,
    }),
    [selectedRepoId]
  )

  const { data: specs, isLoading, isError, error, refetch, isFetching } = useSpecs(filters)

  // Client-side status filter
  const filteredSpecs = useMemo(() => {
    if (!specs) return []

    let filtered = [...specs]

    if (statusFilter.length > 0) {
      filtered = filtered.filter((spec) => statusFilter.includes(spec.status))
    }

    // Sort by updated_at (most recent first)
    filtered.sort(
      (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
    )

    return filtered
  }, [specs, statusFilter])

  const toggleStatusFilter = (status: SpecStatus) => {
    if (statusFilter.includes(status)) {
      setStatusFilter(statusFilter.filter((s) => s !== status))
    } else {
      setStatusFilter([...statusFilter, status])
    }
  }

  // Loading state
  if (isLoading) {
    return (
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Specs</h2>
        </div>
        <div className="space-y-2">
          {Array.from({ length: 3 }).map((_, i) => (
            <div
              key={i}
              className="h-[72px] animate-pulse rounded-lg border border-border bg-muted/50"
            />
          ))}
        </div>
      </div>
    )
  }

  // Error state
  if (isError) {
    return (
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Specs</h2>
        </div>
        <div className="flex flex-col items-center justify-center py-8 text-center">
          <AlertCircle className="h-10 w-10 text-destructive" />
          <h3 className="mt-4 text-lg font-semibold">Failed to load specs</h3>
          <p className="mt-2 text-sm text-muted-foreground max-w-sm">
            {error instanceof Error ? error.message : 'An unexpected error occurred'}
          </p>
          <Button onClick={() => refetch()} className="mt-4" variant="outline">
            <RefreshCw className="mr-2 h-4 w-4" />
            Try again
          </Button>
        </div>
      </div>
    )
  }

  const hasFilters = statusFilter.length > 0

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-semibold">Specs</h2>
          {isFetching && (
            <RefreshCw className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
          )}
        </div>
        <Button size="sm" onClick={openCreateModal}>
          <Plus className="h-4 w-4 mr-1.5" />
          New Spec
        </Button>
      </div>

      {/* Status filter buttons */}
      <div className="flex flex-wrap gap-1.5">
        {FILTER_STATUSES.map((status) => (
          <Button
            key={status}
            variant={statusFilter.includes(status) ? 'default' : 'outline'}
            size="sm"
            className={cn(
              'h-7 text-xs px-2.5',
              statusFilter.includes(status) && 'shadow-sm'
            )}
            onClick={() => toggleStatusFilter(status)}
          >
            {SPEC_STATUS_LABELS[status]}
          </Button>
        ))}
        {hasFilters && (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 text-xs px-2.5 text-muted-foreground"
            onClick={() => setStatusFilter([])}
          >
            Clear
          </Button>
        )}
      </div>

      {/* Spec list */}
      {filteredSpecs.length === 0 ? (
        <EmptyState
          icon={<FileText />}
          title={hasFilters ? 'No specs match your filters' : 'No specs yet'}
          description={
            hasFilters
              ? 'Try adjusting your filters to find specs.'
              : 'Create a new spec to have the AI agent explore the codebase and generate a technical specification for your idea.'
          }
          action={
            hasFilters
              ? {
                  label: 'Clear filters',
                  onClick: () => setStatusFilter([]),
                }
              : {
                  label: 'New Spec',
                  onClick: openCreateModal,
                }
          }
        />
      ) : (
        <div className="space-y-2">
          <div className="text-sm text-muted-foreground">
            {filteredSpecs.length} spec{filteredSpecs.length !== 1 ? 's' : ''}
          </div>
          {filteredSpecs.map((spec) => (
            <SpecListItem
              key={spec.id}
              spec={spec}
              isSelected={spec.id === drawerSpecId}
            />
          ))}
        </div>
      )}
    </div>
  )
}
