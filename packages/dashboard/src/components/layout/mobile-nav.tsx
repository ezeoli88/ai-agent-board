'use client'

import { Link, useLocation } from '@tanstack/react-router'
import {
  ClipboardList,
  FileText,
  Settings,
  Plus,
  TestTube2,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { Button } from '@/components/ui/button'
import { FEATURE_FLAGS } from '@/config/features'
import { useLayoutStore } from '@/stores/layout-store'
import { useTaskUIStore } from '@/features/tasks/stores/task-ui-store'

interface NavItem {
  title: string
  href: string
  icon: React.ComponentType<{ className?: string }>
}

const navItems: NavItem[] = [
  {
    title: 'Tasks',
    href: '/board',
    icon: ClipboardList,
  },
  ...(FEATURE_FLAGS.qaDashboard
    ? [
        {
          title: 'QA',
          href: '/qa',
          icon: TestTube2,
        },
      ]
    : []),
  {
    title: 'Specs',
    href: '/specs',
    icon: FileText,
  },
]

const bottomNavItems: NavItem[] = [
  {
    title: 'Settings',
    href: '/settings',
    icon: Settings,
  },
]

export function MobileNav() {
  const { pathname } = useLocation()
  const { isMobileNavOpen, setMobileNavOpen } = useLayoutStore()
  const { openCreateModal } = useTaskUIStore()

  const handleLinkClick = () => {
    setMobileNavOpen(false)
  }

  const handleCreateTask = () => {
    setMobileNavOpen(false)
    openCreateModal()
  }

  return (
    <Sheet open={isMobileNavOpen} onOpenChange={setMobileNavOpen}>
      <SheetContent
        side="left"
        className="w-[300px] p-0"
        aria-label="Navigation menu"
      >
        <SheetHeader className="border-b border-border px-4 py-4">
          <SheetTitle className="text-left">Navigation</SheetTitle>
          <SheetDescription className="sr-only">
            Main navigation for tasks, specs, and settings.
          </SheetDescription>
        </SheetHeader>

        <div className="flex h-[calc(100vh-5rem)] flex-col">
          {/* Quick action */}
          <div className="p-3 border-b border-border">
            <Button
              onClick={handleCreateTask}
              className="w-full h-11"
              size="lg"
            >
              <Plus className="h-5 w-5 mr-2" />
              New Task
            </Button>
          </div>

          {/* Main navigation */}
          <nav
            className="flex-1 space-y-1 p-3"
            aria-label="Main navigation"
          >
            <div className="mb-2 px-3 py-2">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                Menu
              </h3>
            </div>
            {navItems.map((item) => {
              const isActive = pathname === item.href || pathname.startsWith(`${item.href}/`)
              const Icon = item.icon

              return (
                <Link
                  key={`${item.href}-${item.title}`}
                  to={item.href}
                  onClick={handleLinkClick}
                  className={cn(
                    // Minimum touch target of 44px
                    'flex min-h-[44px] items-center gap-3 rounded-md px-3 text-sm font-medium',
                    'transition-colors duration-150',
                    'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
                    isActive
                      ? 'bg-accent text-accent-foreground'
                      : 'text-foreground hover:bg-accent hover:text-accent-foreground active:bg-accent/80'
                  )}
                  aria-current={isActive ? 'page' : undefined}
                >
                  <Icon className="h-5 w-5 shrink-0" aria-hidden="true" />
                  <span className="truncate">{item.title}</span>
                </Link>
              )
            })}
          </nav>

          {/* Bottom navigation */}
          <div className="border-t border-border p-3 mt-auto">
            {bottomNavItems.map((item) => {
              const isActive = pathname === item.href
              const Icon = item.icon

              return (
                <Link
                  key={item.href}
                  to={item.href}
                  onClick={handleLinkClick}
                  className={cn(
                    // Minimum touch target of 44px
                    'flex min-h-[44px] items-center gap-3 rounded-md px-3 text-sm font-medium',
                    'transition-colors duration-150',
                    'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
                    isActive
                      ? 'bg-accent text-accent-foreground'
                      : 'text-foreground hover:bg-accent hover:text-accent-foreground active:bg-accent/80'
                  )}
                  aria-current={isActive ? 'page' : undefined}
                >
                  <Icon className="h-5 w-5 shrink-0" aria-hidden="true" />
                  <span className="truncate">{item.title}</span>
                </Link>
              )
            })}
          </div>
        </div>
      </SheetContent>
    </Sheet>
  )
}
