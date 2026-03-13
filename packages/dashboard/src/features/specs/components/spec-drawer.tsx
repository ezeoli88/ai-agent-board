'use client'

import { useEffect, useState, useCallback, useRef } from 'react'
import { VisuallyHidden } from 'radix-ui'
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
} from '@/components/ui/sheet'
import {
  Tabs,
  TabsList,
  TabsTrigger,
  TabsContent,
} from '@/components/ui/tabs'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/lib/utils'
import { useSpec } from '../hooks/use-spec'
import { useSpecUIStore } from '../stores/spec-ui-store'
import { SpecStatusBadge } from './spec-status-badge'
import { SpecChat } from './spec-chat'

/**
 * SpecDrawer - A right-side Sheet that shows a quick preview of a spec.
 * Contains two tabs: "Chat" for interacting with the agent, and "Spec"
 * for viewing the draft or final spec content.
 *
 * Controlled by `specUIStore.drawerSpecId`.
 */

const MIN_WIDTH_PX = 512
const MAX_WIDTH_VW = 0.5

export function SpecDrawer() {
  const drawerSpecId = useSpecUIStore((state) => state.drawerSpecId)
  const closeDrawer = useSpecUIStore((state) => state.closeDrawer)

  const isOpen = !!drawerSpecId
  const { data: spec, isLoading } = useSpec(drawerSpecId)

  const [activeTab, setActiveTab] = useState<string>('chat')

  // Resizable drawer width (same pattern as TaskDrawer)
  const [widthPx, setWidthPx] = useState(MIN_WIDTH_PX)
  const isDraggingRef = useRef(false)

  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    isDraggingRef.current = true

    const onMouseMove = (ev: MouseEvent) => {
      if (!isDraggingRef.current) return
      const newWidth = window.innerWidth - ev.clientX
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

  // Close drawer when spec is deleted (spec disappears while drawer is open)
  useEffect(() => {
    if (isOpen && !isLoading && !spec) {
      closeDrawer()
    }
  }, [isOpen, isLoading, spec, closeDrawer])

  // Reset tab to "chat" when a new spec is opened
  useEffect(() => {
    if (drawerSpecId) {
      setActiveTab('chat')
    }
  }, [drawerSpecId])

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      closeDrawer()
    }
  }

  return (
    <Sheet open={isOpen} onOpenChange={handleOpenChange}>
      <SheetContent
        side="right"
        className="sm:!max-w-none w-full flex flex-col p-0 gap-0"
        style={{ width: widthPx }}
      >
        {/* Resize handle on the left edge */}
        <div
          onMouseDown={handleResizeStart}
          className="absolute left-0 top-0 bottom-0 w-1.5 cursor-col-resize z-50 group"
        >
          <div className="h-full w-full transition-colors bg-transparent group-hover:bg-border/60 group-active:bg-border" />
        </div>

        {isLoading || !spec ? (
          <>
            <VisuallyHidden.Root>
              <SheetTitle>Loading spec</SheetTitle>
            </VisuallyHidden.Root>
            <DrawerSkeleton />
          </>
        ) : (
          <>
            {/* Header */}
            <SheetHeader className="px-4 pt-4 pb-3 border-b space-y-2 flex-shrink-0">
              <div className="flex items-center justify-between pr-8">
                <SpecStatusBadge status={spec.status} />
              </div>
              <VisuallyHidden.Root>
                <SheetTitle>{spec.title}</SheetTitle>
                <SheetDescription>Spec details</SheetDescription>
              </VisuallyHidden.Root>
              <h2 className="text-sm font-medium leading-relaxed text-foreground line-clamp-2">
                {spec.title}
              </h2>
              {spec.user_input !== spec.title && (
                <p className="text-xs text-muted-foreground line-clamp-2">
                  {spec.user_input}
                </p>
              )}
            </SheetHeader>

            {/* Tabs */}
            <Tabs
              value={activeTab}
              onValueChange={setActiveTab}
              className="flex flex-col flex-1 min-h-0"
            >
              <div className="border-b flex-shrink-0 px-4">
                <TabsList className="bg-transparent h-9">
                  <TabsTrigger value="chat">Chat</TabsTrigger>
                  <TabsTrigger value="spec">Spec</TabsTrigger>
                </TabsList>
              </div>

              <TabsContent
                value="chat"
                className="flex-1 min-h-0 mt-0"
              >
                <SpecChat
                  spec={spec}
                  onViewSpec={() => setActiveTab('spec')}
                  className="h-full"
                />
              </TabsContent>

              <TabsContent
                value="spec"
                className="flex-1 min-h-0 mt-0"
              >
                <SpecPreview spec={spec} />
              </TabsContent>
            </Tabs>
          </>
        )}
      </SheetContent>
    </Sheet>
  )
}

// ============================================================================
// SpecPreview - read-only preview of draft_spec or final_spec
// ============================================================================

function SpecPreview({ spec }: { spec: NonNullable<ReturnType<typeof useSpec>['data']> }) {
  const content = spec.final_spec || spec.draft_spec

  if (!content) {
    return (
      <div className="flex items-center justify-center h-48 text-muted-foreground">
        <div className="text-center space-y-1">
          <p className="text-sm">No spec generated yet</p>
          <p className="text-xs text-muted-foreground/70">
            Use the Chat tab to start refining with the agent
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex-1 overflow-y-auto h-full">
      <div className="p-4">
        <div className="prose prose-sm dark:prose-invert max-w-none">
          <MarkdownContent content={content} />
        </div>
      </div>
    </div>
  )
}

// ============================================================================
// MarkdownContent - lightweight markdown renderer
// ============================================================================

/**
 * Renders markdown content as formatted HTML. Uses a simple approach
 * with pre-formatted text and basic styling. For a full-featured markdown
 * renderer, a library like react-markdown could be used, but this keeps
 * the bundle lean since specs are mostly structured text.
 */
function MarkdownContent({ content }: { content: string }) {
  return (
    <pre className={cn(
      'whitespace-pre-wrap font-sans text-sm leading-relaxed',
      'text-foreground'
    )}>
      {content}
    </pre>
  )
}

// ============================================================================
// DrawerSkeleton
// ============================================================================

function DrawerSkeleton() {
  return (
    <div className="p-4 space-y-4">
      {/* Status badge */}
      <div className="flex items-center justify-between">
        <Skeleton className="h-5 w-20" />
      </div>
      {/* Title */}
      <Skeleton className="h-5 w-3/4" />
      {/* Description */}
      <div className="space-y-2">
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-2/3" />
      </div>
      {/* Tab area */}
      <div className="pt-4 space-y-3">
        <div className="flex gap-2">
          <Skeleton className="h-8 w-16" />
          <Skeleton className="h-8 w-16" />
        </div>
        <Skeleton className="h-48 w-full" />
      </div>
    </div>
  )
}
