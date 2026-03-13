export type SpecStatus = 'draft' | 'refining' | 'approved' | 'failed' | 'canceled'

export interface Spec {
  id: string
  repository_id: string
  title: string
  user_input: string
  draft_spec: string | null
  final_spec: string | null
  agent_type: string | null
  agent_model: string | null
  status: SpecStatus
  task_id: string | null
  created_at: string
  updated_at: string
  approved_at: string | null
}

export interface CreateSpecInput {
  repository_id: string
  user_input: string
  title?: string
  agent_type?: string
  agent_model?: string
}

export interface UpdateSpecInput {
  title?: string
  final_spec?: string | null
}

export const SPEC_STATUS_LABELS: Record<SpecStatus, string> = {
  draft: 'Draft',
  refining: 'Refining',
  approved: 'Approved',
  failed: 'Failed',
  canceled: 'Canceled',
}

export const SPEC_STATUS_COLORS: Record<SpecStatus, string> = {
  draft: 'bg-gray-500/10 text-gray-500',
  refining: 'bg-blue-500/10 text-blue-500',
  approved: 'bg-green-500/10 text-green-500',
  failed: 'bg-red-500/10 text-red-500',
  canceled: 'bg-gray-500/10 text-gray-400',
}
