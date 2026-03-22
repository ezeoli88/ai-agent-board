export type { Spec, SpecStatus, CreateSpecInput, UpdateSpecInput } from '@dash-agent/shared'
export { SPEC_STATUSES } from '@dash-agent/shared'
import type { SpecStatus } from '@dash-agent/shared'

export const SPEC_STATUS_LABELS: Record<SpecStatus, string> = {
  draft: 'Draft',
  generating: 'Generating',
  ready: 'Ready',
  failed: 'Failed',
  cancelled: 'Cancelled',
}

export const SPEC_STATUS_COLORS: Record<SpecStatus, string> = {
  draft: 'bg-gray-500/10 text-gray-500',
  generating: 'bg-blue-500/10 text-blue-500',
  ready: 'bg-green-500/10 text-green-500',
  failed: 'bg-red-500/10 text-red-500',
  cancelled: 'bg-gray-500/10 text-gray-400',
}
