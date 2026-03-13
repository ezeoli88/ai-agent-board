'use client'

import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { SpecStatus } from '../types'

interface SpecUIState {
  // Filter state
  statusFilter: SpecStatus[]
  setStatusFilter: (statuses: SpecStatus[]) => void

  // Drawer state (ephemeral, not persisted)
  drawerSpecId: string | null
  openDrawer: (specId: string) => void
  closeDrawer: () => void

  // Create modal state (ephemeral, not persisted)
  isCreateModalOpen: boolean
  openCreateModal: () => void
  closeCreateModal: () => void
}

// Only persist filter preferences
interface PersistedState {
  statusFilter: SpecStatus[]
}

export const useSpecUIStore = create<SpecUIState>()(
  persist(
    (set) => ({
      // Filter state
      statusFilter: [],
      setStatusFilter: (statuses) => set({ statusFilter: statuses }),

      // Drawer state
      drawerSpecId: null,
      openDrawer: (specId) => set({ drawerSpecId: specId }),
      closeDrawer: () => set({ drawerSpecId: null }),

      // Create modal state
      isCreateModalOpen: false,
      openCreateModal: () => set({ isCreateModalOpen: true }),
      closeCreateModal: () => set({ isCreateModalOpen: false }),
    }),
    {
      name: 'dash-agent-spec-ui',
      partialize: (state): PersistedState => ({
        statusFilter: state.statusFilter,
      }),
    }
  )
)
