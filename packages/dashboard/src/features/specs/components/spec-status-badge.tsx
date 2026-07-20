'use client'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import { SPEC_STATUS_COLORS, SPEC_STATUS_LABELS, type SpecStatus } from '../types'

interface SpecStatusBadgeProps {
  status: SpecStatus
  className?: string
}

export function SpecStatusBadge({ status, className }: SpecStatusBadgeProps) {
  return (
    <Badge variant="secondary" className={cn(SPEC_STATUS_COLORS[status], className)}>
      {SPEC_STATUS_LABELS[status]}
    </Badge>
  )
}
