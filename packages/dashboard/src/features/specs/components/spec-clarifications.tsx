'use client'

import { useEffect, useState } from 'react'
import { HelpCircle, Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { useRolePermissions } from '@/features/settings'
import { cn } from '@/lib/utils'
import { useSpecActions } from '../hooks/use-spec-actions'
import type { Spec } from '../types'

interface SpecClarificationsProps {
  spec: Spec
  compact?: boolean
  className?: string
}

const EMPTY_STRINGS: string[] = []

export function SpecClarifications({ spec, compact = false, className }: SpecClarificationsProps) {
  const actions = useSpecActions(spec.id)
  const { canManageSpecs } = useRolePermissions()
  const clarificationQuestions = Array.isArray(spec.clarification_questions)
    ? spec.clarification_questions
    : EMPTY_STRINGS
  const clarificationAnswers = Array.isArray(spec.clarification_answers)
    ? spec.clarification_answers
    : EMPTY_STRINGS
  const [answers, setAnswers] = useState<string[]>(clarificationAnswers)

  useEffect(() => {
    setAnswers(
      clarificationQuestions.map((_, index) =>
        typeof clarificationAnswers[index] === 'string' ? clarificationAnswers[index] : ''
      )
    )
  }, [clarificationAnswers, clarificationQuestions])

  if (clarificationQuestions.length === 0) {
    return null
  }

  const isEditable = canManageSpecs && spec.status === 'clarifying'
  const normalizedAnswers = clarificationQuestions.map((_, index) =>
    typeof answers[index] === 'string' ? answers[index] : ''
  )
  const canSubmit =
    isEditable &&
    normalizedAnswers.some((answer) => answer.trim().length > 0) &&
    !actions.answerClarifications.isPending

  const setAnswer = (index: number, value: string) => {
    setAnswers((prev) => {
      const next = clarificationQuestions.map((_, questionIndex) =>
        typeof prev[questionIndex] === 'string' ? prev[questionIndex] : ''
      )
      next[index] = value
      return next
    })
  }

  return (
    <section className={cn('space-y-3 rounded-md border bg-muted/20 p-3', className)}>
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <HelpCircle className="h-4 w-4 text-primary" />
          <h3 className="text-sm font-semibold">Clarification Questions</h3>
        </div>
        {isEditable && (
          <Button
            size="sm"
            onClick={() => actions.answerClarifications.mutate(normalizedAnswers)}
            disabled={!canSubmit}
          >
            {actions.answerClarifications.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : null}
            Generate Spec
          </Button>
        )}
      </div>

      <div className={cn('space-y-3', compact && 'max-h-80 overflow-y-auto pr-2')}>
        {clarificationQuestions.map((question, index) => (
          <div key={`${index}-${question}`} className="space-y-1.5">
            <Label htmlFor={`spec-clarification-${spec.id}-${index}`} className="text-xs leading-relaxed">
              {index + 1}. {question}
            </Label>
            {isEditable ? (
              <Textarea
                id={`spec-clarification-${spec.id}-${index}`}
                value={answers[index] ?? ''}
                onChange={(event) => setAnswer(index, event.target.value)}
                className="min-h-[72px] resize-y text-sm"
                placeholder="Answer this question..."
              />
            ) : (
              <p className="whitespace-pre-wrap rounded-md border bg-background p-2 text-sm text-muted-foreground">
                {clarificationAnswers[index]?.trim() || 'No answer provided.'}
              </p>
            )}
          </div>
        ))}
      </div>
    </section>
  )
}
