'use client'

import { useEffect } from 'react'
import { useRouter } from '@tanstack/react-router'
import { Header } from './header'
import { Sidebar } from './sidebar'
import { useRepoContext } from '@/features/repos/hooks/use-repo-context'
import { useLayoutStore } from '@/stores/layout-store'

interface MainLayoutProps {
  children: React.ReactNode
}

export function MainLayout({ children }: MainLayoutProps) {
  const router = useRouter()
  const { selectedRepoId, hasRepos, isLoading } = useRepoContext()
  const { isSidebarCollapsed } = useLayoutStore()

  // Redirect to repo selection when no repo is selected or no repos available
  useEffect(() => {
    if (!isLoading && (!hasRepos || !selectedRepoId)) {
      router.navigate({ to: '/repos' })
    }
  }, [isLoading, hasRepos, selectedRepoId, router])

  return (
    <div className="relative min-h-screen bg-gradient-page">
      <Header />
      <div className="flex">
        <Sidebar />
        <main
          id="main-content"
          tabIndex={-1}
          aria-label="Main content"
          className={isSidebarCollapsed ? 'ml-16' : 'ml-[280px]'}
          style={{ transition: 'margin-left 0.2s ease' }}
        >
          <div className="w-full py-6 px-4 md:px-6 lg:px-8">
            {children}
          </div>
        </main>
      </div>
    </div>
  )
}
