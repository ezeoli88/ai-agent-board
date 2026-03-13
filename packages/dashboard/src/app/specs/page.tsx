import { SpecList, SpecDrawer, CreateSpecDialog } from '@/features/specs'

export default function SpecsPage() {
  return (
    <>
      <div className="animate-in fade-in duration-300">
        <SpecList />
      </div>
      <CreateSpecDialog />
      <SpecDrawer />
    </>
  )
}
