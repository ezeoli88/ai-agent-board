'use client'

import { Link } from '@tanstack/react-router'
import {
  AlertTriangle,
  CheckCircle2,
  FileText,
  GitBranch,
  ListChecks,
  Loader2,
  Play,
  RefreshCw,
  Route,
  Trash2,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { useRolePermissions } from '@/features/settings'
import { useSpecActions } from '../hooks/use-spec-actions'
import type { Spec } from '../types'

interface SpecActionsProps {
  spec: Spec
  variant?: 'full' | 'compact'
}

export function SpecActions({ spec, variant = 'full' }: SpecActionsProps) {
  const { canManageSpecs } = useRolePermissions()

  if (!canManageSpecs) {
    return null
  }

  return <SpecActionsContent spec={spec} variant={variant} />
}

function SpecActionsContent({ spec, variant = 'full' }: SpecActionsProps) {
  const isCompact = variant === 'compact'
  const actions = useSpecActions(spec.id)

  const isPending =
    actions.generateSpec.isPending ||
    actions.answerClarifications.isPending ||
    actions.approveSpec.isPending ||
    actions.generatePlan.isPending ||
    actions.approvePlan.isPending ||
    actions.generateTasks.isPending ||
    actions.approveTasks.isPending ||
    actions.createTasks.isPending ||
    actions.deleteSpec.isPending

  const btnClass = isCompact ? '' : 'w-full justify-start'
  const btnSize = isCompact ? 'sm' : 'default'

  const renderPrimaryAction = () => {
    switch (spec.status) {
      case 'idea':
      case 'failed':
        return (
          <Button
            size={btnSize}
            className={btnClass}
            onClick={() => actions.generateSpec.mutate()}
            disabled={isPending}
          >
            {actions.generateSpec.isPending ? <Loader2 className="h-4 w-4 animate-spin" /> : <FileText className="h-4 w-4" />}
            Generate Spec
          </Button>
        )
      case 'spec_draft':
        return (
          <Button
            size={btnSize}
            className={btnClass}
            onClick={() => actions.approveSpec.mutate(spec.final_spec ?? spec.generated_spec ?? undefined)}
            disabled={isPending}
          >
            {actions.approveSpec.isPending ? <Loader2 className="h-4 w-4 animate-spin" /> : <CheckCircle2 className="h-4 w-4" />}
            Approve Spec
          </Button>
        )
      case 'spec_review':
        return (
          <Button
            size={btnSize}
            className={btnClass}
            onClick={() => actions.generatePlan.mutate()}
            disabled={isPending}
          >
            {actions.generatePlan.isPending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Route className="h-4 w-4" />}
            Generate Plan
          </Button>
        )
      case 'plan':
        if (!spec.plan_approved_at) {
          return (
            <Button
              size={btnSize}
              className={btnClass}
              onClick={() => actions.approvePlan.mutate(spec.final_plan ?? spec.generated_plan ?? undefined)}
              disabled={isPending}
            >
              {actions.approvePlan.isPending ? <Loader2 className="h-4 w-4 animate-spin" /> : <CheckCircle2 className="h-4 w-4" />}
              Approve Plan
            </Button>
          )
        }
        return (
          <Button
            size={btnSize}
            className={btnClass}
            onClick={() => actions.generateTasks.mutate()}
            disabled={isPending}
          >
            {actions.generateTasks.isPending ? <Loader2 className="h-4 w-4 animate-spin" /> : <ListChecks className="h-4 w-4" />}
            Generate Breakdown
          </Button>
        )
      case 'task_breakdown':
        return (
          <Button
            size={btnSize}
            className={btnClass}
            onClick={() => actions.approveTasks.mutate(spec.final_tasks ?? spec.generated_tasks ?? undefined)}
            disabled={isPending}
          >
            {actions.approveTasks.isPending ? <Loader2 className="h-4 w-4 animate-spin" /> : <CheckCircle2 className="h-4 w-4" />}
            Approve Breakdown
          </Button>
        )
      case 'ready_for_implementation':
        return (
          <Button
            size={btnSize}
            className={btnClass}
            onClick={() => actions.createTasks.mutate()}
            disabled={isPending}
          >
            {actions.createTasks.isPending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
            Create Task from Spec
          </Button>
        )
      case 'implemented':
        return (
          <Button size={btnSize} className={btnClass} variant="outline" asChild>
            <Link to="/board">
              <GitBranch className="h-4 w-4" />
              View Tasks
            </Link>
          </Button>
        )
      default:
        return null
    }
  }

  const primary = renderPrimaryAction()

  return (
    <div className={isCompact ? 'flex flex-wrap gap-2' : 'space-y-2'}>
      {primary}

      {(spec.status === 'spec_draft' || spec.status === 'task_breakdown') && (
        <Button
          size={btnSize}
          variant="outline"
          className={btnClass}
          onClick={() => {
            if (spec.status === 'spec_draft') actions.generateSpec.mutate()
            if (spec.status === 'task_breakdown') actions.generateTasks.mutate()
          }}
          disabled={isPending}
        >
          <RefreshCw className="h-4 w-4" />
          Regenerate
        </Button>
      )}

      {spec.error && (
        <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/10 p-2 text-xs text-destructive">
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <span>{spec.error}</span>
        </div>
      )}

      {spec.status !== 'implemented' && (
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button size={btnSize} variant="destructive" className={btnClass} disabled={isPending}>
              <Trash2 className="h-4 w-4" />
              Delete Spec
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Delete Spec</AlertDialogTitle>
              <AlertDialogDescription>
                This removes the spec and its generated SDD artifacts. Created tasks are not deleted.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                onClick={() => actions.deleteSpec.mutate()}
              >
                Delete Spec
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      )}
    </div>
  )
}
