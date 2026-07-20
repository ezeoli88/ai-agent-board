'use client'

import { QABoardView } from '@/features/qa'
import { TaskDrawer } from '@/features/tasks/components'

export default function QAPage() {
  return (
    <>
      <div className="animate-in fade-in duration-300">
        <QABoardView />
      </div>
      <TaskDrawer />
    </>
  )
}
