'use client'

import { Link, useParams } from '@tanstack/react-router'
import { AlertCircle, ChevronRight, FileQuestion } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { ApiClientError } from '@/lib/api-client'
import { useSpec } from '@/features/specs/hooks/use-spec'
import { SpecDetail } from '@/features/specs/components/spec-detail'

export default function SpecDetailPage() {
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
      <SpecDetail spec={spec} />
    </div>
  )
}

function Breadcrumb({ specId, specTitle }: { specId: string; specTitle?: string }) {
  return (
    <nav className="flex items-center space-x-1 text-sm text-muted-foreground">
      <Link to="/specs" className="transition-colors hover:text-foreground">
        Specs
      </Link>
      <ChevronRight className="h-4 w-4" />
      <span className="max-w-xs truncate font-medium text-foreground">
        {specTitle || `Spec ${specId}`}
      </span>
    </nav>
  )
}

function SpecDetailSkeleton() {
  return (
    <div className="grid gap-6 lg:grid-cols-3">
      <div className="space-y-6 lg:col-span-2">
        <Skeleton className="h-40 w-full" />
        <Skeleton className="h-[520px] w-full" />
      </div>
      <div className="space-y-4">
        <Skeleton className="h-48 w-full" />
        <Skeleton className="h-32 w-full" />
      </div>
    </div>
  )
}

function NotFoundState({ specId }: { specId: string }) {
  return (
    <Card>
      <CardContent className="flex flex-col items-center justify-center py-16">
        <FileQuestion className="mb-4 h-12 w-12 text-muted-foreground" />
        <h2 className="mb-2 text-xl font-semibold">Spec Not Found</h2>
        <p className="mb-6 max-w-md text-center text-muted-foreground">
          The spec with ID &quot;{specId}&quot; could not be found.
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
        <AlertCircle className="mb-4 h-12 w-12 text-destructive" />
        <h2 className="mb-2 text-xl font-semibold">Error Loading Spec</h2>
        <p className="mb-2 max-w-md text-center text-muted-foreground">
          There was a problem loading the spec details.
        </p>
        <p className="mb-6 font-mono text-sm text-destructive">{error.message}</p>
        <Button variant="outline" onClick={() => window.location.reload()}>
          Try Again
        </Button>
      </CardContent>
    </Card>
  )
}
