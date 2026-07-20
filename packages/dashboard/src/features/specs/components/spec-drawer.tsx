'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { VisuallyHidden } from 'radix-ui'
import { ExternalLink } from 'lucide-react'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { ScrollArea } from '@/components/ui/scroll-area'
import { useRolePermissions } from '@/features/settings'
import { useSpec } from '../hooks/use-spec'
import { useSpecUIStore } from '../stores/spec-ui-store'
import { SpecActions } from './spec-actions'
import { SpecChat } from './spec-chat'
import { SpecClarifications } from './spec-clarifications'
import { SpecStatusBadge } from './spec-status-badge'

const MIN_WIDTH_PX = 512
const MAX_WIDTH_VW = 0.5

export function SpecDrawer() {
  const drawerSpecId = useSpecUIStore((state) => state.drawerSpecId)
  const closeDrawer = useSpecUIStore((state) => state.closeDrawer)
  const isOpen = !!drawerSpecId
  const { data: spec, isLoading } = useSpec(drawerSpecId ?? '')
  const { canManageSpecs } = useRolePermissions()

  const [widthPx, setWidthPx] = useState(MIN_WIDTH_PX)
  const isDraggingRef = useRef(false)

  const handleResizeStart = useCallback((event: React.MouseEvent) => {
    event.preventDefault()
    isDraggingRef.current = true

    const onMouseMove = (moveEvent: MouseEvent) => {
      if (!isDraggingRef.current) return
      const newWidth = window.innerWidth - moveEvent.clientX
      const maxPx = window.innerWidth * MAX_WIDTH_VW
      setWidthPx(Math.min(Math.max(newWidth, MIN_WIDTH_PX), maxPx))
    }

    const onMouseUp = () => {
      isDraggingRef.current = false
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
    }

    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
  }, [])

  useEffect(() => {
    if (isOpen && !isLoading && !spec) {
      closeDrawer()
    }
  }, [isOpen, isLoading, spec, closeDrawer])

  return (
    <Sheet open={isOpen} onOpenChange={(open) => !open && closeDrawer()}>
      <SheetContent
        side="right"
        className="sm:!max-w-none w-full flex flex-col p-0 gap-0"
        style={{ width: widthPx }}
      >
        <div
          onMouseDown={handleResizeStart}
          className="absolute left-0 top-0 bottom-0 z-50 w-1.5 cursor-col-resize group"
        >
          <div className="h-full w-full bg-transparent transition-colors group-hover:bg-border/60 group-active:bg-border" />
        </div>

        {isLoading || !spec ? (
          <>
            <VisuallyHidden.Root>
              <SheetTitle>Loading spec</SheetTitle>
              <SheetDescription>Loading spec details</SheetDescription>
            </VisuallyHidden.Root>
            <DrawerSkeleton />
          </>
        ) : (
          <>
            <SheetHeader className="space-y-3 border-b px-4 pb-3 pt-4">
              <div className="flex items-center justify-between pr-8">
                <SpecStatusBadge status={spec.status} />
                <Button variant="outline" size="sm" asChild>
                  <Link to="/specs/$specId" params={{ specId: spec.id }}>
                    <ExternalLink className="h-4 w-4" />
                    Open
                  </Link>
                </Button>
              </div>
              <VisuallyHidden.Root>
                <SheetTitle>{spec.title}</SheetTitle>
                <SheetDescription>Spec details</SheetDescription>
              </VisuallyHidden.Root>
              <div>
                <h2 className="text-base font-semibold leading-tight">{spec.title}</h2>
                <p className="mt-2 line-clamp-4 whitespace-pre-wrap text-sm text-muted-foreground">
                  {spec.user_input}
                </p>
              </div>
            </SheetHeader>

            {canManageSpecs && (
              <div className="border-b px-4 py-3">
                <SpecActions spec={spec} variant="compact" />
              </div>
            )}

            <div className="flex min-h-0 flex-1 flex-col">
              <ScrollArea className="max-h-72 border-b">
                <div className="space-y-5 p-4">
                  <SpecClarifications spec={spec} compact />
                  <PreviewSection title="Spec" content={spec.final_spec ?? spec.generated_spec} />
                  <PreviewSection title="Plan" content={spec.final_plan ?? spec.generated_plan} />
                  <PreviewSection title="Task Breakdown" content={spec.final_tasks ?? spec.generated_tasks} />
                </div>
              </ScrollArea>
              <SpecChat spec={spec} className="flex-1" />
            </div>
          </>
        )}
      </SheetContent>
    </Sheet>
  )
}

function PreviewSection({ title, content }: { title: string; content?: string | null }) {
  return (
    <section className="space-y-2">
      <h3 className="text-sm font-semibold">{title}</h3>
      <div className="max-h-80 overflow-y-auto overscroll-contain rounded-md border bg-muted/30 p-3 pr-4">
        {content ? (
          <p className="whitespace-pre-wrap text-xs leading-relaxed text-foreground">{content}</p>
        ) : (
          <p className="text-xs text-muted-foreground">Not generated yet.</p>
        )}
      </div>
    </section>
  )
}

function DrawerSkeleton() {
  return (
    <div className="space-y-4 p-4">
      <div className="flex items-center justify-between">
        <Skeleton className="h-5 w-24" />
        <Skeleton className="h-8 w-20" />
      </div>
      <Skeleton className="h-5 w-3/4" />
      <Skeleton className="h-16 w-full" />
      <div className="flex gap-2">
        <Skeleton className="h-8 w-28" />
        <Skeleton className="h-8 w-28" />
      </div>
      <Skeleton className="h-48 w-full" />
    </div>
  )
}
