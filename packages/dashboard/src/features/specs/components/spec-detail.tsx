'use client'

import { Link, useParams, useRouter } from '@tanstack/react-router'
import { ArrowLeft, AlertCircle, FileQuestion, RefreshCw, Loader2, Play, Rocket } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { toast } from 'sonner'
import { useSpec } from '../hooks/use-spec'
import { useUpdateSpec } from '../hooks/use-spec-mutations'
import { useApproveSpec, useRefineSpec } from '../hooks/use-spec-actions'
import { useRepos } from '@/features/repos/hooks/use-repos'
import { SpecStatusBadge } from './spec-status-badge'
import { SpecEditor } from './spec-editor'
import { InlineEdit } from '@/features/tasks/components/inline-edit'
import { useSpecUIStore } from '../stores/spec-ui-store'
import { ApiClientError } from '@/lib/api-client'
import type { Spec } from '../types'

export function SpecDetail() {
  const { specId } = useParams({ strict: false }) as { specId: string }
  const { data: spec, isLoading, error } = useSpec(specId)

  if (isLoading) {
    return (
      <div className="space-y-6">
        <Breadcrumb specId={specId} />
        <SpecDetailSkeleton />
      </div>
    )
  }

  if (error instanceof ApiClientError && error.statusCode === 404) {
    return (
      <div className="space-y-6">
        <Breadcrumb specId={specId} />
        <NotFoundState specId={specId} />
      </div>
    )
  }

  if (error) {
    return (
      <div className="space-y-6">
        <Breadcrumb specId={specId} />
        <ErrorState error={error} />
      </div>
    )
  }

  if (!spec) {
    return (
      <div className="space-y-6">
        <Breadcrumb specId={specId} />
        <NotFoundState specId={specId} />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <Breadcrumb specId={specId} specTitle={spec.title} />
      <SpecDetailContent spec={spec} />
    </div>
  )
}

// -- Internal components --

interface SpecDetailContentProps {
  spec: Spec
}

function SpecDetailContent({ spec }: SpecDetailContentProps) {
  const router = useRouter()
  const updateSpec = useUpdateSpec()
  const approveSpec = useApproveSpec()
  const refineSpec = useRefineSpec()
  const openDrawer = useSpecUIStore((state) => state.openDrawer)
  const { data: repos } = useRepos()

  // Resolve repo name from repository_id
  const repo = repos?.find((r) => r.id === spec.repository_id)
  const repoName = repo?.name ?? spec.repository_id

  // Determine edit permissions based on status
  const isTerminal = spec.status === 'approved' || spec.status === 'failed' || spec.status === 'canceled'
  const isRefining = spec.status === 'refining'
  const isTitleEditable = !isTerminal && !isRefining
  const isContentEditable = !isTerminal && !isRefining

  // The content to show: prefer final_spec (user-edited), fall back to draft_spec
  const specContent = spec.final_spec ?? spec.draft_spec

  const handleSaveTitle = async (newTitle: string) => {
    try {
      await updateSpec.mutateAsync({
        id: spec.id,
        input: { title: newTitle },
      })
      toast.success('Title updated')
    } catch (error) {
      console.error('Failed to update title:', error)
      toast.error('Failed to update title', {
        description: error instanceof Error ? error.message : 'An unexpected error occurred',
      })
      throw error // re-throw so InlineEdit stays in edit mode
    }
  }

  const handleContinueRefining = () => {
    refineSpec.mutate(spec.id, {
      onSuccess: () => {
        toast.success('Refining started')
        // Navigate to /specs and open drawer so user sees the chat/logs
        router.navigate({ to: '/specs' })
        // Small delay to let the route mount before opening the drawer
        setTimeout(() => openDrawer(spec.id), 100)
      },
      onError: (error) => {
        toast.error('Failed to start refining', {
          description: error instanceof Error ? error.message : 'An unexpected error occurred',
        })
      },
    })
  }

  const handleSendToCode = () => {
    approveSpec.mutate(spec.id, {
      onSuccess: (data) => {
        const taskData = data as { task?: { id?: string } }
        const taskId = taskData?.task?.id

        toast.success('Spec approved! Task created.', {
          description: taskId ? 'Navigate to the task to start coding.' : undefined,
          action: taskId
            ? {
                label: 'View Task',
                onClick: () => router.navigate({ to: `/board` }),
              }
            : undefined,
        })
      },
      onError: (error) => {
        toast.error('Failed to approve spec', {
          description: error instanceof Error ? error.message : 'An unexpected error occurred',
        })
      },
    })
  }

  return (
    <div className="space-y-6">
      {/* Header: Title, status, repo */}
      <div className="space-y-2">
        <div className="text-2xl font-bold tracking-tight lg:text-3xl">
          <InlineEdit
            value={spec.title}
            onSave={handleSaveTitle}
            isSaving={updateSpec.isPending}
            disabled={!isTitleEditable}
            minLength={1}
            placeholder="Enter spec title..."
            inputClassName="text-2xl lg:text-3xl font-bold tracking-tight h-auto py-1"
          />
        </div>
        <div className="flex items-center gap-3">
          <SpecStatusBadge status={spec.status} />
          <span className="text-sm text-muted-foreground">{repoName}</span>
          {isRefining && (
            <span className="inline-flex items-center gap-1.5 text-sm text-blue-500">
              <Loader2 className="size-3.5 animate-spin" />
              Agent is refining...
            </span>
          )}
        </div>
      </div>

      {/* Body: Editor */}
      <SpecEditor
        specId={spec.id}
        content={specContent}
        readOnly={!isContentEditable}
      />

      {/* Action bar */}
      {!isTerminal && (
        <Card>
          <CardHeader className="py-3">
            <CardTitle className="text-lg">Actions</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            <Button
              variant="outline"
              className="w-full justify-start"
              onClick={handleContinueRefining}
              disabled={isRefining || refineSpec.isPending || approveSpec.isPending}
            >
              {refineSpec.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <RefreshCw className="h-4 w-4" />
              )}
              {refineSpec.isPending ? 'Starting...' : 'Continue Refining'}
            </Button>

            <Button
              className="w-full justify-start"
              onClick={handleSendToCode}
              disabled={!specContent || isRefining || approveSpec.isPending || refineSpec.isPending}
            >
              {approveSpec.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Rocket className="h-4 w-4" />
              )}
              {approveSpec.isPending ? 'Creating task...' : 'Send to Code'}
            </Button>

            {!specContent && (
              <p className="text-xs text-muted-foreground">
                The spec needs content before it can be sent to code. Use "Continue Refining" to
                let the agent generate a draft, or switch to Edit mode to write one manually.
              </p>
            )}
          </CardContent>
        </Card>
      )}

      {/* Show approved info */}
      {spec.status === 'approved' && spec.task_id && (
        <Card>
          <CardContent className="py-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Play className="h-4 w-4 text-green-500" />
                <span>This spec has been approved and a task was created.</span>
              </div>
              <Button variant="outline" size="sm" asChild>
                <Link to="/board">
                  View Board
                </Link>
              </Button>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  )
}

function Breadcrumb({ specId, specTitle }: { specId: string; specTitle?: string }) {
  return (
    <div className="flex items-center justify-between">
      <nav className="flex items-center space-x-1 text-sm text-muted-foreground">
        <Link to="/specs" className="hover:text-foreground transition-colors">
          Specs
        </Link>
        <span className="mx-1">/</span>
        <span className="text-foreground font-medium truncate max-w-xs">
          {specTitle || `Spec ${specId.slice(0, 8)}...`}
        </span>
      </nav>
      <Button variant="ghost" size="sm" asChild>
        <Link to="/specs" className="gap-2">
          <ArrowLeft className="h-4 w-4" />
          Back to Specs
        </Link>
      </Button>
    </div>
  )
}

function SpecDetailSkeleton() {
  return (
    <div className="space-y-6">
      {/* Title skeleton */}
      <div className="space-y-2">
        <Skeleton className="h-9 w-2/3" />
        <div className="flex items-center gap-3">
          <Skeleton className="h-5 w-16" />
          <Skeleton className="h-4 w-24" />
        </div>
      </div>
      {/* Editor skeleton */}
      <Card>
        <CardHeader className="border-b py-2 px-4">
          <div className="flex gap-2">
            <Skeleton className="h-7 w-14" />
            <Skeleton className="h-7 w-18" />
          </div>
        </CardHeader>
        <CardContent className="p-4">
          <div className="space-y-3">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-5/6" />
            <Skeleton className="h-4 w-4/6" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-3/4" />
          </div>
        </CardContent>
      </Card>
      {/* Actions skeleton */}
      <Card>
        <CardHeader className="py-3">
          <Skeleton className="h-6 w-20" />
        </CardHeader>
        <CardContent className="space-y-2">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </CardContent>
      </Card>
    </div>
  )
}

function NotFoundState({ specId }: { specId: string }) {
  return (
    <Card>
      <CardContent className="flex flex-col items-center justify-center py-16">
        <FileQuestion className="h-12 w-12 text-muted-foreground mb-4" />
        <h2 className="text-xl font-semibold mb-2">Spec Not Found</h2>
        <p className="text-muted-foreground text-center mb-6 max-w-md">
          The spec with ID &quot;{specId}&quot; could not be found. It may have been deleted.
        </p>
        <Button asChild>
          <Link to="/specs">Back to Specs</Link>
        </Button>
      </CardContent>
    </Card>
  )
}

function ErrorState({ error }: { error: Error }) {
  return (
    <Card>
      <CardContent className="flex flex-col items-center justify-center py-16">
        <AlertCircle className="h-12 w-12 text-destructive mb-4" />
        <h2 className="text-xl font-semibold mb-2">Error Loading Spec</h2>
        <p className="text-muted-foreground text-center mb-2 max-w-md">
          There was a problem loading the spec details.
        </p>
        <p className="text-sm text-destructive mb-6 font-mono">
          {error.message}
        </p>
        <div className="flex gap-3">
          <Button variant="outline" onClick={() => window.location.reload()}>
            Try Again
          </Button>
          <Button asChild>
            <Link to="/specs">Back to Specs</Link>
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
