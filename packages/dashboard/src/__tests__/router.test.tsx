import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, act } from '@testing-library/react'
import { http, HttpResponse } from 'msw'
import { server } from '@/test/msw-server'

/**
 * Tests for router guards (beforeLoad) in router.tsx.
 *
 * Approach: We mock all page components and heavy dependencies to keep the
 * tests focused on route guard behavior. MSW controls the /api/repos response
 * to trigger redirects or allow navigation.
 */

// ---------------------------------------------------------------------------
// Mock all page components to avoid rendering the full app tree
// ---------------------------------------------------------------------------

vi.mock('@/components/shared/providers', () => ({
  Providers: ({ children }: { children: React.ReactNode }) => (
    <div data-testid="providers">{children}</div>
  ),
}))

vi.mock('@/components/layout/main-layout', () => ({
  MainLayout: ({ children }: { children: React.ReactNode }) => (
    <div data-testid="main-layout">{children}</div>
  ),
}))

vi.mock('@/app/page', () => ({
  default: () => <div data-testid="home-page">Home Page</div>,
}))

vi.mock('@/app/board/page', () => ({
  default: () => <div data-testid="board-page">Board Page</div>,
}))

vi.mock('@/app/diff/[taskId]/page', () => ({
  default: () => <div data-testid="diff-page">Diff Page</div>,
}))

vi.mock('@/app/qa/page', () => ({
  default: () => <div data-testid="qa-page">QA Page</div>,
}))

vi.mock('@/app/qa/runs/[runId]/page', () => ({
  default: () => <div data-testid="qa-run-detail-page">QA Run Detail Page</div>,
}))

vi.mock('@/app/specs/page', () => ({
  default: () => <div data-testid="specs-page">Specs Page</div>,
}))

vi.mock('@/app/specs/[specId]/page', () => ({
  default: () => <div data-testid="spec-detail-page">Spec Detail Page</div>,
}))

vi.mock('@/app/repos/page', () => ({
  default: () => <div data-testid="repos-page">Repos Page</div>,
}))

vi.mock('@/app/settings/page', () => ({
  default: () => <div data-testid="settings-page">Settings Page</div>,
}))

vi.mock('@/lib/auth', () => ({
  getAuthToken: vi.fn(() => 'test-token'),
}))

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function mockReposEndpoint(repos: unknown[], status = 200) {
  server.use(
    http.get('*/api/repos', () => {
      if (status >= 400) {
        return HttpResponse.json({ error: 'Server error' }, { status })
      }
      return HttpResponse.json(repos)
    }),
  )
}

/**
 * Imports a fresh router instance. Since TanStack Router is stateful,
 * we reset modules and re-import to get a clean slate per test.
 */
async function createFreshRouter(initialPath: string) {
  // Dynamic import for fresh module each time
  const { createRouter, createMemoryHistory } = await import('@tanstack/react-router')

  // Import the route tree definition from the actual router module
  const routerModule = await import('../router')

  // TanStack Router v1 does not allow re-creating from an existing router's route tree
  // directly. Instead, we build a new router from the exported one's route tree.
  // The `router` export includes the full route tree with all guards.
  const freshRouter = createRouter({
    routeTree: routerModule.router.routeTree,
    history: createMemoryHistory({ initialEntries: [initialPath] }),
  })

  return freshRouter
}

async function renderRouter(initialPath: string) {
  const { RouterProvider } = await import('@tanstack/react-router')
  const freshRouter = await createFreshRouter(initialPath)

  let result: ReturnType<typeof render>
  await act(async () => {
    result = render(<RouterProvider router={freshRouter} />)
  })

  return { ...result!, router: freshRouter }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('Router guards', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('mainLayoutRoute beforeLoad', () => {
    it('allows navigation to /board when repos exist', async () => {
      mockReposEndpoint([{ id: 'repo-1', name: 'test-repo' }])

      await renderRouter('/board')

      await waitFor(
        () => {
          expect(screen.getByTestId('board-page')).toBeInTheDocument()
        },
        { timeout: 5000 },
      )
    })

    it('redirects to /repos when repos are empty', async () => {
      mockReposEndpoint([])

      await renderRouter('/board')

      await waitFor(
        () => {
          expect(screen.getByTestId('repos-page')).toBeInTheDocument()
        },
        { timeout: 5000 },
      )
    })

    it('redirects to /repos when API returns error', async () => {
      mockReposEndpoint([], 500)

      await renderRouter('/board')

      await waitFor(
        () => {
          expect(screen.getByTestId('repos-page')).toBeInTheDocument()
        },
        { timeout: 5000 },
      )
    })

    it('redirects to /repos when fetch throws network error', async () => {
      server.use(
        http.get('*/api/repos', () => {
          return HttpResponse.error()
        }),
      )

      await renderRouter('/board')

      await waitFor(
        () => {
          expect(screen.getByTestId('repos-page')).toBeInTheDocument()
        },
        { timeout: 5000 },
      )
    })

    it('allows navigation to /settings when repos exist', async () => {
      mockReposEndpoint([{ id: 'repo-1', name: 'test-repo' }])

      await renderRouter('/settings')

      await waitFor(
        () => {
          expect(screen.getByTestId('settings-page')).toBeInTheDocument()
        },
        { timeout: 5000 },
      )
    })

    it('allows navigation to /qa when QA dashboard is enabled', async () => {
      mockReposEndpoint([{ id: 'repo-1', name: 'test-repo' }])

      await renderRouter('/qa')

      await waitFor(
        () => {
          expect(screen.getByTestId('qa-page')).toBeInTheDocument()
        },
        { timeout: 5000 },
      )
    })
  })

  describe('setupRoute beforeLoad', () => {
    it('always redirects /setup to /repos', async () => {
      // Even with repos available, /setup should redirect
      mockReposEndpoint([{ id: 'repo-1', name: 'test-repo' }])

      await renderRouter('/setup')

      await waitFor(
        () => {
          expect(screen.getByTestId('repos-page')).toBeInTheDocument()
        },
        { timeout: 5000 },
      )
    })
  })

  describe('reposRoute', () => {
    it('renders repos page directly (no guard)', async () => {
      // repos route has no beforeLoad guard
      mockReposEndpoint([])

      await renderRouter('/repos')

      await waitFor(
        () => {
          expect(screen.getByTestId('repos-page')).toBeInTheDocument()
        },
        { timeout: 5000 },
      )
    })
  })
})
