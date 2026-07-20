'use client'

import { useEffect, useState } from 'react'
import { Calendar, FileText, GitBranch, ListChecks, Route } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import { useRolePermissions } from '@/features/settings'
import { formatRelativeTime } from '@/lib/formatters'
import { useUpdateSpec } from '../hooks/use-update-spec'
import { SpecActions } from './spec-actions'
import { SpecChat } from './spec-chat'
import { SpecClarifications } from './spec-clarifications'
import { SpecStatusBadge } from './spec-status-badge'
import type { Spec, UpdateSpecInput } from '../types'

interface SpecDetailProps {
  spec: Spec
}

export function SpecDetail({ spec }: SpecDetailProps) {
  const { canManageSpecs, isSpecReadOnly } = useRolePermissions()

  return (
    <div className="grid gap-6 lg:grid-cols-3">
      <div className="space-y-6 lg:col-span-2">
        <Card>
          <CardHeader className="space-y-3">
            <div className="flex items-center justify-between gap-3">
              <SpecStatusBadge status={spec.status} />
              <span className="text-xs text-muted-foreground">{formatRelativeTime(spec.updated_at)}</span>
            </div>
            <CardTitle className="text-2xl">{spec.title}</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="whitespace-pre-wrap text-sm leading-relaxed text-muted-foreground">
              {spec.user_input}
            </p>
          </CardContent>
        </Card>

        <SpecClarifications spec={spec} />

        <Tabs defaultValue="spec" className="w-full">
          <TabsList variant="line" className="mb-4">
            <TabsTrigger value="spec" className="gap-1.5">
              <FileText className="h-4 w-4" />
              Spec
            </TabsTrigger>
            <TabsTrigger value="plan" className="gap-1.5">
              <Route className="h-4 w-4" />
              Plan
            </TabsTrigger>
            <TabsTrigger value="tasks" className="gap-1.5">
              <ListChecks className="h-4 w-4" />
              Tasks
            </TabsTrigger>
          </TabsList>

          <TabsContent value="spec" className="mt-0">
            <SpecContentEditor
              spec={spec}
              title="Spec Draft"
              field="final_spec"
              value={spec.final_spec ?? spec.generated_spec ?? ''}
              placeholder="Generate a spec draft to review it here."
              readOnly={isSpecReadOnly}
            />
          </TabsContent>

          <TabsContent value="plan" className="mt-0">
            <SpecContentEditor
              spec={spec}
              title="Plan"
              field="final_plan"
              value={spec.final_plan ?? spec.generated_plan ?? ''}
              placeholder="Approve the spec, then generate a plan."
              readOnly={isSpecReadOnly}
            />
          </TabsContent>

          <TabsContent value="tasks" className="mt-0">
            <SpecContentEditor
              spec={spec}
              title="Task Breakdown"
              field="final_tasks"
              value={spec.final_tasks ?? spec.generated_tasks ?? ''}
              placeholder="Approve the plan, then generate a task breakdown."
              readOnly={isSpecReadOnly}
            />
          </TabsContent>
        </Tabs>
      </div>

      <div className="space-y-4">
        {canManageSpecs && (
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">Actions</CardTitle>
            </CardHeader>
            <CardContent>
              <SpecActions spec={spec} />
            </CardContent>
          </Card>
        )}

        <Card>
          <CardContent className="h-[520px] p-0">
            <SpecChat spec={spec} />
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Metadata</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <div className="flex items-center justify-between gap-3">
              <span className="inline-flex items-center gap-2 text-muted-foreground">
                <Calendar className="h-4 w-4" />
                Created
              </span>
              <span>{formatRelativeTime(spec.created_at)}</span>
            </div>
            <div className="flex items-center justify-between gap-3">
              <span className="inline-flex items-center gap-2 text-muted-foreground">
                <GitBranch className="h-4 w-4" />
                Tasks
              </span>
              <span>{spec.task_ids.length}</span>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}

interface SpecContentEditorProps {
  spec: Spec
  title: string
  field: 'final_spec' | 'final_plan' | 'final_tasks'
  value: string
  placeholder: string
  readOnly?: boolean
}

function SpecContentEditor({ spec, title, field, value, placeholder, readOnly = false }: SpecContentEditorProps) {
  const updateSpec = useUpdateSpec(spec.id)
  const [draft, setDraft] = useState(value)

  useEffect(() => {
    setDraft(value)
  }, [value])

  const isDirty = draft !== value

  const handleSave = async () => {
    const payload = { [field]: draft } as UpdateSpecInput
    await updateSpec.mutateAsync(payload)
    toast.success(`${title} saved`)
  }

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0">
        <CardTitle className="text-lg">{title}</CardTitle>
        {!readOnly && (
          <Button
            size="sm"
            onClick={handleSave}
            disabled={!isDirty || updateSpec.isPending}
          >
            Save
          </Button>
        )}
      </CardHeader>
      <CardContent>
        <Textarea
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder={placeholder}
          className="min-h-[420px] resize-y font-mono text-sm leading-relaxed"
          readOnly={readOnly}
        />
      </CardContent>
    </Card>
  )
}
