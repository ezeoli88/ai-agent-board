'use client'

import { CreateSpecDialog, SpecBoardView, SpecDrawer } from '@/features/specs/components'

export default function SpecsPage() {
  return (
    <>
      <div className="animate-in fade-in duration-300">
        <SpecBoardView />
      </div>
      <CreateSpecDialog />
      <SpecDrawer />
    </>
  )
}
