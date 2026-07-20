'use client'

import { Link } from '@tanstack/react-router'
import { Menu, Plus, Layers, Settings } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ThemeToggle } from './theme-toggle'
import { RoleSelector } from '@/features/settings'
import { useTaskUIStore } from '@/features/tasks/stores/task-ui-store'
import { useLayoutStore } from '@/stores/layout-store'

export function Header() {
  const { openCreateModal } = useTaskUIStore()
  const setMobileNavOpen = useLayoutStore((state) => state.setMobileNavOpen)

  return (
    <header className="sticky top-0 z-50 w-full border-b border-border bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="flex h-14 items-center px-4 md:px-6">
        <Button
          variant="ghost"
          size="icon"
          className="mr-2 h-9 w-9 md:hidden"
          onClick={() => setMobileNavOpen(true)}
          aria-label="Open navigation menu"
        >
          <Menu className="h-5 w-5" />
        </Button>

        {/* Logo */}
        <div className="flex items-center">
          <Link
            className="flex items-center gap-2 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 rounded-md"
            to="/"
          >
            <Layers className="h-6 w-6 text-primary" aria-hidden="true" />
            <span className="font-bold text-lg hidden sm:inline-block">
              Agent Board
            </span>
          </Link>
        </div>

        {/* Right side actions */}
        <div className="ml-auto flex items-center gap-2">
          <RoleSelector compact showLabel={false} />

          {/* Create task button */}
          <Button
            size="sm"
            className="hidden sm:inline-flex items-center gap-2"
            onClick={openCreateModal}
          >
            <Plus className="h-4 w-4" />
            <span>New Task</span>
          </Button>
          <Button
            size="icon"
            className="h-9 w-9 sm:hidden"
            onClick={openCreateModal}
            aria-label="Create new task"
          >
            <Plus className="h-5 w-5" />
          </Button>

          {/* Settings */}
          <Button
            variant="ghost"
            size="icon"
            className="h-9 w-9"
            asChild
          >
            <Link to="/settings" aria-label="Settings">
              <Settings className="h-4 w-4" />
            </Link>
          </Button>

          {/* Theme toggle */}
          <ThemeToggle />
        </div>
      </div>
    </header>
  )
}
