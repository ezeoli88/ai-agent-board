import type { SpecStatus } from '../schemas/spec.schema.js';

export const SPEC_STATUS_LABELS: Record<SpecStatus, string> = {
  idea: 'Idea',
  clarifying: 'Clarifying',
  spec_draft: 'Spec Draft',
  spec_review: 'Spec Review',
  plan: 'Plan',
  task_breakdown: 'Task Breakdown',
  ready_for_implementation: 'Ready for Implementation',
  implemented: 'Implemented',
  failed: 'Failed',
};

export const SPEC_STATUS_COLORS: Record<SpecStatus, string> = {
  idea: 'bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-100',
  clarifying: 'bg-sky-100 text-sky-800 dark:bg-sky-900 dark:text-sky-100',
  spec_draft: 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-100',
  spec_review: 'bg-cyan-100 text-cyan-800 dark:bg-cyan-900 dark:text-cyan-100',
  plan: 'bg-violet-100 text-violet-800 dark:bg-violet-900 dark:text-violet-100',
  task_breakdown: 'bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100',
  ready_for_implementation: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100',
  implemented: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-100',
  failed: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100',
};

export function isSpecTerminalStatus(status: SpecStatus): boolean {
  return status === 'implemented' || status === 'failed';
}

export function getSpecAvailableActions(status: SpecStatus): readonly string[] {
  switch (status) {
    case 'idea':
      return ['generate_clarifications', 'edit', 'delete'] as const;
    case 'clarifying':
      return ['answer_clarifications', 'delete'] as const;
    case 'spec_draft':
      return ['approve_spec', 'regenerate_spec', 'edit_spec', 'delete'] as const;
    case 'spec_review':
      return ['generate_plan', 'delete'] as const;
    case 'plan':
      return ['approve_plan', 'generate_tasks', 'delete'] as const;
    case 'task_breakdown':
      return ['approve_tasks', 'regenerate_tasks', 'delete'] as const;
    case 'ready_for_implementation':
      return ['create_tasks', 'delete'] as const;
    case 'implemented':
      return ['view_tasks'] as const;
    case 'failed':
      return ['retry', 'delete'] as const;
    default:
      return [] as const;
  }
}
