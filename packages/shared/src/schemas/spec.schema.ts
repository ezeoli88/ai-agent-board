import { z } from 'zod';
import { AgentTypeSchema } from './agent.schema.js';

// ============================================================================
// Spec Status
// ============================================================================

export const SPEC_STATUSES = [
  'draft',
  'generating',
  'ready',
  'failed',
  'cancelled',
] as const;

export const SpecStatusSchema = z.enum(SPEC_STATUSES);
export type SpecStatus = z.infer<typeof SpecStatusSchema>;

// ============================================================================
// Spec Entity
// ============================================================================

export const SpecSchema = z.object({
  id: z.string(),
  repository_id: z.string(),
  title: z.string(),
  user_input: z.string(),
  draft_spec: z.string().nullable(),
  final_spec: z.string().nullable(),
  agent_type: z.string().nullable().optional(),
  agent_model: z.string().nullable().optional(),
  status: SpecStatusSchema,
  task_id: z.string().nullable(),
  created_at: z.string(),
  updated_at: z.string(),
  approved_at: z.string().nullable().optional(),
});
export type Spec = z.infer<typeof SpecSchema>;

// ============================================================================
// Spec Input Schemas
// ============================================================================

export const CreateSpecSchema = z.object({
  repository_id: z.string().min(1, 'Repository is required'),
  user_input: z.string().min(1, 'Description is required'),
  title: z.string().max(200).optional(),
  agent_type: AgentTypeSchema.optional(),
  agent_model: z.string().optional(),
});
export type CreateSpecInput = z.infer<typeof CreateSpecSchema>;

export const UpdateSpecSchema = z.object({
  title: z.string().min(1).optional(),
  final_spec: z.string().nullable().optional(),
});
export type UpdateSpecInput = z.infer<typeof UpdateSpecSchema>;
