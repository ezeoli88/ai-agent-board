import { z } from 'zod';
import { AgentTypeSchema } from './agent.schema.js';

export const SPEC_STATUSES = [
  'idea',
  'clarifying',
  'spec_draft',
  'spec_review',
  'plan',
  'task_breakdown',
  'ready_for_implementation',
  'implemented',
  'failed',
] as const;

export const SpecStatusSchema = z.enum(SPEC_STATUSES);
export type SpecStatus = z.infer<typeof SpecStatusSchema>;

export const SpecSchema = z.object({
  id: z.string().uuid(),
  repository_id: z.string().uuid(),
  title: z.string().min(1),
  user_input: z.string().min(1),
  status: SpecStatusSchema,

  clarification_questions: z.array(z.string()).default([]),
  clarification_answers: z.array(z.string()).default([]),
  clarification_answered_at: z.string().datetime().nullable(),

  generated_spec: z.string().nullable(),
  final_spec: z.string().nullable(),
  spec_approved_at: z.string().datetime().nullable(),

  generated_plan: z.string().nullable(),
  final_plan: z.string().nullable(),
  plan_approved_at: z.string().datetime().nullable(),

  generated_tasks: z.string().nullable(),
  final_tasks: z.string().nullable(),
  tasks_approved_at: z.string().datetime().nullable(),
  task_ids: z.array(z.string().uuid()).default([]),

  agent_type: AgentTypeSchema.nullable().optional(),
  agent_model: z.string().nullable().optional(),
  agent_session_id: z.string().nullable().optional(),
  error: z.string().nullable(),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type Spec = z.infer<typeof SpecSchema>;

export const CreateSpecSchema = z.object({
  repository_id: z.string().uuid('Repository is required'),
  user_input: z.string().min(1, 'Idea is required'),
  title: z.string().optional(),
  agent_type: AgentTypeSchema.optional(),
  agent_model: z.string().optional(),
});
export type CreateSpecInput = z.infer<typeof CreateSpecSchema>;

export const UpdateSpecSchema = z.object({
  title: z.string().min(1).optional(),
  user_input: z.string().min(1).optional(),
  status: SpecStatusSchema.optional(),
  clarification_questions: z.array(z.string()).optional(),
  clarification_answers: z.array(z.string()).optional(),
  clarification_answered_at: z.string().datetime().nullable().optional(),
  generated_spec: z.string().nullable().optional(),
  final_spec: z.string().nullable().optional(),
  spec_approved_at: z.string().datetime().nullable().optional(),
  generated_plan: z.string().nullable().optional(),
  final_plan: z.string().nullable().optional(),
  plan_approved_at: z.string().datetime().nullable().optional(),
  generated_tasks: z.string().nullable().optional(),
  final_tasks: z.string().nullable().optional(),
  tasks_approved_at: z.string().datetime().nullable().optional(),
  task_ids: z.array(z.string().uuid()).optional(),
  agent_type: AgentTypeSchema.nullable().optional(),
  agent_model: z.string().nullable().optional(),
  agent_session_id: z.string().nullable().optional(),
  error: z.string().nullable().optional(),
});
export type UpdateSpecInput = z.infer<typeof UpdateSpecSchema>;

export const ApproveSpecStepSchema = z.object({
  content: z.string().optional(),
});
export type ApproveSpecStepInput = z.infer<typeof ApproveSpecStepSchema>;

export const AnswerClarificationsSchema = z.object({
  answers: z.array(z.string()).max(5),
});
export type AnswerClarificationsInput = z.infer<typeof AnswerClarificationsSchema>;

export const CreateTasksFromSpecResponseSchema = z.object({
  spec: SpecSchema,
  task_ids: z.array(z.string().uuid()),
});
export type CreateTasksFromSpecResponse = z.infer<typeof CreateTasksFromSpecResponseSchema>;
