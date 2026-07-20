'use client'

import { Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { useRolePermissions } from '@/features/settings'
import { cn } from '@/lib/utils'
import { useSpecUIStore } from '../stores/spec-ui-store'
import { SpecCard } from './spec-card'
import type { Spec } from '../types'
import type { SpecColumnConfig } from './spec-board-types'

interface SpecColumnProps {
  config: SpecColumnConfig
  specs: Spec[]
}

export function SpecColumn({ config, specs }: SpecColumnProps) {
  const openCreateModal = useSpecUIStore((state) => state.openCreateModal)
  const { canManageSpecs } = useRolePermissions()

  return (
    <div
      className={cn(
        'flex h-full min-w-[280px] max-w-[320px] flex-col rounded-lg border',
        config.borderColor,
        config.bgColor,
      )}
    >
      <div className="flex items-center justify-between border-b border-inherit px-3 py-2.5">
        <div className="flex items-center gap-2">
          <h3 className={cn('text-sm font-semibold', config.color)}>{config.title}</h3>
          <span
            className={cn(
              'inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-background/80 px-1.5 text-xs font-medium',
              config.color,
            )}
          >
            {specs.length}
          </span>
        </div>
        {canManageSpecs && config.id === 'idea' && (
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={openCreateModal}
            aria-label="Add new spec"
          >
            <Plus className="h-4 w-4" />
          </Button>
        )}
      </div>

      <ScrollArea className="flex-1 p-2">
        <div className="space-y-2 pb-2">
          {specs.length === 0 ? (
            <div className="flex h-20 items-center justify-center rounded-md border border-dashed border-muted-foreground/25">
              <p className="text-xs text-muted-foreground">No specs</p>
            </div>
          ) : (
            specs.map((spec) => <SpecCard key={spec.id} spec={spec} />)
          )}
        </div>
      </ScrollArea>
    </div>
  )
}

export function SpecColumnSkeleton() {
  return (
    <div className="flex h-full min-w-[280px] max-w-[320px] flex-col rounded-lg border bg-muted/30 animate-pulse">
      <div className="flex items-center justify-between border-b px-3 py-2.5">
        <div className="h-4 w-24 rounded bg-muted" />
        <div className="h-5 w-5 rounded-full bg-muted" />
      </div>
      <div className="flex-1 space-y-2 p-2">
        <div className="h-28 rounded-lg bg-muted" />
        <div className="h-28 rounded-lg bg-muted" />
      </div>
    </div>
  )
}
