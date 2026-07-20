import { createRouter, createRoute, createRootRoute, Outlet, redirect } from '@tanstack/react-router'
import { Providers } from '@/components/shared/providers'
import { MainLayout } from '@/components/layout/main-layout'
import { FEATURE_FLAGS } from '@/config/features'
import { getAuthToken } from '@/lib/auth'

// Lazy imports for pages to enable code splitting
import HomePage from '@/app/page'
import BoardPage from '@/app/board/page'
import DiffPage from '@/app/diff/[taskId]/page'
import QAPage from '@/app/qa/page'
import QARunDetailPage from '@/app/qa/runs/[runId]/page'
import SpecsPage from '@/app/specs/page'
import SpecDetailPage from '@/app/specs/[specId]/page'
import ReposPage from '@/app/repos/page'
import SettingsPage from '@/app/settings/page'
import McpSetupPage from '@/app/mcp-setup/page'

// Root route - wraps everything with Providers
const rootRoute = createRootRoute({
  component: () => (
    <Providers>
      <Outlet />
    </Providers>
  ),
})

// Layout route for pages that use MainLayout (Header + content)
const mainLayoutRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'main-layout',
  beforeLoad: async () => {
    // Check if repos are available before rendering any main page
    const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || window.location.origin
    const authToken = getAuthToken()
    const headers: HeadersInit = {
      'Content-Type': 'application/json',
      ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
    }

    let repos: unknown[] = []
    try {
      const response = await fetch(`${API_BASE_URL}/api/repos`, { headers, cache: 'no-store' })
      if (response.ok) {
        repos = await response.json()
      }
    } catch {
      // Network error - redirect to repos
    }

    if (!repos || repos.length === 0) {
      throw redirect({ to: '/repos' })
    }
  },
  component: () => (
    <MainLayout>
      <Outlet />
    </MainLayout>
  ),
})

// Index route - smart redirect based on repos
const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: HomePage,
})

// Board route
const boardRoute = createRoute({
  getParentRoute: () => mainLayoutRoute,
  path: '/board',
  component: BoardPage,
})

const qaRoute = createRoute({
  getParentRoute: () => mainLayoutRoute,
  path: '/qa',
  beforeLoad: () => {
    if (!FEATURE_FLAGS.qaDashboard) {
      throw redirect({ to: '/board' })
    }
  },
  component: QAPage,
})

const qaRunDetailRoute = createRoute({
  getParentRoute: () => mainLayoutRoute,
  path: '/qa/runs/$runId',
  beforeLoad: () => {
    if (!FEATURE_FLAGS.qaDashboard) {
      throw redirect({ to: '/board' })
    }
  },
  component: QARunDetailPage,
})

const specsRoute = createRoute({
  getParentRoute: () => mainLayoutRoute,
  path: '/specs',
  component: SpecsPage,
})

const specDetailRoute = createRoute({
  getParentRoute: () => mainLayoutRoute,
  path: '/specs/$specId',
  component: SpecDetailPage,
})

// Diff route
const diffRoute = createRoute({
  getParentRoute: () => mainLayoutRoute,
  path: '/diff/$taskId',
  component: DiffPage,
})

// Repos route (no MainLayout - has its own full-page layout)
const reposRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/repos',
  component: ReposPage,
})

// Setup route - redirect to /repos
const setupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/setup',
  beforeLoad: () => {
    throw redirect({ to: '/repos' })
  },
})

// Settings route
const settingsRoute = createRoute({
  getParentRoute: () => mainLayoutRoute,
  path: '/settings',
  component: SettingsPage,
})

// MCP Setup route (standalone - no repo guard, accessible from /repos)
const mcpSetupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/mcp-setup',
  component: McpSetupPage,
})

// Build route tree
const routeTree = rootRoute.addChildren([
  indexRoute,
  reposRoute,
  setupRoute,
  mcpSetupRoute,
  mainLayoutRoute.addChildren([
    boardRoute,
    qaRoute,
    qaRunDetailRoute,
    specsRoute,
    specDetailRoute,
    diffRoute,
    settingsRoute,
  ]),
])

// Create and export router
export const router = createRouter({ routeTree })

// Type registration for TanStack Router
declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
