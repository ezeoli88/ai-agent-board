'use client'

import { Link, useLocation } from '@tanstack/react-router'
import {
  ChevronLeft,
  ChevronRight,
  LayoutDashboard,
  LayoutGrid,
  ListTodo,
  FileText,
  Settings,
  Plus,
  FolderGit2,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { ScrollArea } from '@/components/ui/scroll-area'
import { useLayoutStore } from '@/stores/layout-store'
import { TaskList, TaskFilters } from '@/features/tasks/components'
import { RepoList, AddRepoDialog, RepoConfigDialog, useRepoStore } from '@/features/repos'

interface NavItem {
  title: string
  href: string
  icon: React.ComponentType<{ className?: string }>
}

const navItems: NavItem[] = [
  {
    title: 'Board',
    href: '/board',
    icon: LayoutGrid,
  },
  {
    title: 'Specs',
    href: '/specs',
    icon: FileText,
  },
  {
    title: 'Tasks',
    href: '/tasks',
    icon: ListTodo,
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
  const { openAddDialog } = useRepoStore()

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

        {/* Repos and Tasks section - only show when expanded */}
        {!isSidebarCollapsed && (
          <div className="flex flex-1 flex-col overflow-hidden">
            {/* Repos Section */}
            <div className="border-t border-border">
              <div className="flex items-center justify-between px-4 py-2">
                <h3 className="text-xs font-semibold uppercase text-muted-foreground">
                  Repositorios
                </h3>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6"
                      onClick={openAddDialog}
                    >
                      <Plus className="h-4 w-4" />
                      <span className="sr-only">Agregar repositorio</span>
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Agregar repositorio</TooltipContent>
                </Tooltip>
              </div>
              <div className="px-2 pb-2">
                <RepoList compact />
              </div>
            </div>

            {/* Tasks Section */}
            <div className="flex flex-1 flex-col border-t border-border overflow-hidden">
              {/* Compact filters */}
              <div className="p-3 border-b border-border">
                <TaskFilters compact />
              </div>

              {/* Scrollable task list */}
              <ScrollArea className="flex-1">
                <div className="p-2">
                  <TaskList compact />
                </div>
              </ScrollArea>
            </div>
          </div>
        )}

        {/* Collapsed repos button */}
        {isSidebarCollapsed && (
          <div className="p-2">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-10 w-10"
                  onClick={openAddDialog}
                >
                  <FolderGit2 className="h-5 w-5" />
                  <span className="sr-only">Repositorios</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent side="right">Repositorios</TooltipContent>
            </Tooltip>
          </div>
        )}

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

      {/* Dialogs */}
      <AddRepoDialog />
      <RepoConfigDialog />
    </TooltipProvider>
  )
}
