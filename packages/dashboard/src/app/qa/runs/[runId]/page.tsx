'use client'

import { useParams } from '@tanstack/react-router'
import { QARunDetail } from '@/features/qa'

export default function QARunDetailPage() {
  const { runId } = useParams({ strict: false }) as { runId: string }
  return <QARunDetail runId={runId} />
}
