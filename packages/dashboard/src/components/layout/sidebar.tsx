'use client'

import { Link, useLocation } from '@tanstack/react-router'
import {
  ChevronLeft,
  ChevronRight,
  FileText,
  ListTodo,
  Settings,
  TestTube2,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { FEATURE_FLAGS } from '@/config/features'
import { useLayoutStore } from '@/stores/layout-store'

interface NavItem {
  title: string
  href: string
  icon: React.ComponentType<{ className?: string }>
}

const navItems: NavItem[] = [
  {
    title: 'Tasks',
    href: '/board',
    icon: ListTodo,
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

interface SidebarProps {
  className?: string
}

export function Sidebar({ className }: SidebarProps) {
  const { pathname } = useLocation()
  const { isSidebarCollapsed, toggleSidebar } = useLayoutStore()

  return (
    <TooltipProvider delayDuration={0}>
      <aside
        className={cn(
          'relative flex h-full flex-col border-r border-border bg-gradient-sidebar transition-all duration-300 ease-in-out',
          isSidebarCollapsed ? 'w-16' : 'w-[280px]',
          className
        )}
      >
        {/* Collapse toggle button */}
        <Button
          variant="ghost"
          size="icon"
          className="absolute -right-3 top-6 z-10 h-6 w-6 rounded-full border border-border bg-background shadow-sm hover:bg-accent"
          onClick={toggleSidebar}
        >
          {isSidebarCollapsed ? (
            <ChevronRight className="h-3 w-3" />
          ) : (
            <ChevronLeft className="h-3 w-3" />
          )}
          <span className="sr-only">
            {isSidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          </span>
        </Button>

        {/* Navigation header */}
        <div className="flex h-14 items-center border-b border-border px-4">
          {!isSidebarCollapsed && (
            <h2 className="text-sm font-semibold text-sidebar-foreground">
              Agent Board
            </h2>
          )}
        </div>

        {/* Main navigation */}
        <nav className="space-y-1 p-2">
          {navItems.map((item) => {
            const isActive = pathname === item.href || pathname.startsWith(`${item.href}/`)
            const Icon = item.icon

            if (isSidebarCollapsed) {
              return (
                <Tooltip key={`${item.href}-${item.title}`}>
                  <TooltipTrigger asChild>
                    <Link
                      to={item.href}
                      className={cn(
                        'flex h-10 w-10 items-center justify-center rounded-md transition-colors',
                        isActive
                          ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                          : 'text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'
                      )}
                    >
                      <Icon className="h-5 w-5" />
                      <span className="sr-only">{item.title}</span>
                    </Link>
                  </TooltipTrigger>
                  <TooltipContent side="right">
                    {item.title}
                  </TooltipContent>
                </Tooltip>
              )
            }

            return (
              <Link
                key={`${item.href}-${item.title}`}
                to={item.href}
                className={cn(
                  'flex h-10 items-center gap-3 rounded-md px-3 text-sm font-medium transition-colors',
                  isActive
                    ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                    : 'text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'
                )}
              >
                <Icon className="h-5 w-5 shrink-0" />
                <span className="truncate">{item.title}</span>
              </Link>
            )
          })}
        </nav>

        <div className="flex-1" />

        {/* Bottom navigation */}
        <div className="border-t border-border p-2 mt-auto">
          {bottomNavItems.map((item) => {
            const isActive = pathname === item.href
            const Icon = item.icon

            if (isSidebarCollapsed) {
              return (
                <Tooltip key={item.href}>
                  <TooltipTrigger asChild>
                    <Link
                      to={item.href}
                      className={cn(
                        'flex h-10 w-10 items-center justify-center rounded-md transition-colors',
                        isActive
                          ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                          : 'text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'
                      )}
                    >
                      <Icon className="h-5 w-5" />
                      <span className="sr-only">{item.title}</span>
                    </Link>
                  </TooltipTrigger>
                  <TooltipContent side="right">
                    {item.title}
                  </TooltipContent>
                </Tooltip>
              )
            }

            return (
              <Link
                key={item.href}
                to={item.href}
                className={cn(
                  'flex h-10 items-center gap-3 rounded-md px-3 text-sm font-medium transition-colors',
                  isActive
                    ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                    : 'text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'
                )}
              >
                <Icon className="h-5 w-5 shrink-0" />
                <span className="truncate">{item.title}</span>
              </Link>
            )
          })}
        </div>
      </aside>

    </TooltipProvider>
  )
}
