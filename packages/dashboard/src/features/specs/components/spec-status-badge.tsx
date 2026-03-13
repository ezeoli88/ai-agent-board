import { Badge } from '@/components/ui/badge'
import type { SpecStatus } from '../types'
import { SPEC_STATUS_LABELS } from '../types'

const STATUS_VARIANT_MAP: Record<
  SpecStatus,
  'default' | 'secondary' | 'destructive' | 'success'
> = {
  draft: 'secondary',
  refining: 'default',
  approved: 'success',
  failed: 'destructive',
  canceled: 'secondary',
}

interface SpecStatusBadgeProps {
  status: SpecStatus
  className?: string
}

export function SpecStatusBadge({ status, className }: SpecStatusBadgeProps) {
  const label = SPEC_STATUS_LABELS[status]
  const variant = STATUS_VARIANT_MAP[status]

  if (!label) {
    console.error(`SpecStatusBadge: Unknown status "${status}"`)
    return (
      <Badge variant="secondary" className={className}>
        {status || 'Unknown'}
      </Badge>
    )
  }

  return (
    <Badge variant={variant} className={className}>
      {label}
    </Badge>
  )
}
