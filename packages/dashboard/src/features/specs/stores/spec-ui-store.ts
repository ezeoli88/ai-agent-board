'use client'

import { create } from 'zustand'

interface SpecUIState {
  isCreateModalOpen: boolean
  openCreateModal: () => void
  closeCreateModal: () => void

  drawerSpecId: string | null
  openDrawer: (specId: string) => void
  closeDrawer: () => void
}

export const useSpecUIStore = create<SpecUIState>((set) => ({
  isCreateModalOpen: false,
  openCreateModal: () => set({ isCreateModalOpen: true }),
  closeCreateModal: () => set({ isCreateModalOpen: false }),

  drawerSpecId: null,
  openDrawer: (specId) => set({ drawerSpecId: specId }),
  closeDrawer: () => set({ drawerSpecId: null }),
}))
