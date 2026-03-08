# Spec Agent Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add collaborative spec generation via a Spec Agent that reads the repo, generates a technical spec draft, iterates with the user via chat, and creates a Task on approval.

**Architecture:** New `specs` table + CRUD service + routes in Rust backend. New `features/specs/` module in React frontend. Reuses existing `agent_service`, `SSEEmitter`, and CLI agent infrastructure unchanged.

**Tech Stack:** Rust (Axum, rusqlite, tokio) backend. React 19 + TanStack Router/Query + Zustand + shadcn/ui frontend.

---

## Task 1: Backend — Spec model + migration

**Files:**
- Create: `packages/server-rs/src/models/spec.rs`
- Modify: `packages/server-rs/src/models/mod.rs`
- Modify: `packages/server-rs/src/db/migrations.rs`

**Step 1: Create the Spec model**

Create `packages/server-rs/src/models/spec.rs` with:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecStatus {
    #[serde(rename = "draft")]
    Draft,
    #[serde(rename = "refining")]
    Refining,
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "canceled")]
    Canceled,
}

impl SpecStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Refining => "refining",
            Self::Approved => "approved",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }
}

impl std::fmt::Display for SpecStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SpecStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(Self::Draft),
            "refining" => Ok(Self::Refining),
            "approved" => Ok(Self::Approved),
            "failed" => Ok(Self::Failed),
            "canceled" => Ok(Self::Canceled),
            other => Err(format!("unknown spec status: '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub id: String,
    pub repository_id: String,
    pub title: String,
    pub user_input: String,
    pub draft_spec: Option<String>,
    pub final_spec: Option<String>,
    pub agent_type: Option<String>,
    pub agent_model: Option<String>,
    pub status: SpecStatus,
    pub task_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub approved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpecInput {
    pub repository_id: String,
    pub user_input: String,
    pub title: Option<String>,
    pub agent_type: Option<String>,
    pub agent_model: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateSpecInput {
    pub title: Option<String>,
    pub draft_spec: Option<Option<String>>,
    pub final_spec: Option<Option<String>>,
    pub status: Option<SpecStatus>,
    pub task_id: Option<Option<String>>,
    pub approved_at: Option<Option<String>>,
    pub agent_type: Option<Option<String>>,
    pub agent_model: Option<Option<String>>,
}
```

**Step 2: Register the model module**

In `packages/server-rs/src/models/mod.rs`, add after the `task` line:

```rust
pub mod spec;

// In the re-exports section, add:
pub use spec::{CreateSpecInput, Spec, SpecStatus, UpdateSpecInput};
```

**Step 3: Add migration 12**

In `packages/server-rs/src/db/migrations.rs`, add after the migration 11 entry in the `migrations` vec:

```rust
Migration {
    version: 12,
    description: "Create specs table for Spec Agent feature",
    sql: "
        CREATE TABLE IF NOT EXISTS specs (
            id TEXT PRIMARY KEY,
            repository_id TEXT NOT NULL,
            title TEXT NOT NULL,
            user_input TEXT NOT NULL,
            draft_spec TEXT,
            final_spec TEXT,
            agent_type TEXT,
            agent_model TEXT,
            status TEXT DEFAULT 'draft',
            task_id TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
            approved_at TEXT,
            FOREIGN KEY (repository_id) REFERENCES repositories(id),
            FOREIGN KEY (task_id) REFERENCES tasks(id)
        );

        CREATE INDEX IF NOT EXISTS idx_specs_repository_id ON specs(repository_id);
        CREATE INDEX IF NOT EXISTS idx_specs_status ON specs(status);
    ",
},
```

**Step 4: Build to verify**

Run: `cd packages/server-rs && cargo build 2>&1 | tail -5`
Expected: Build succeeds.

**Step 5: Commit**

```bash
git add packages/server-rs/src/models/spec.rs packages/server-rs/src/models/mod.rs packages/server-rs/src/db/migrations.rs
git commit -m "feat(server): add Spec model and migration 12"
```

---

## Task 2: Backend — Spec service (CRUD)

**Files:**
- Create: `packages/server-rs/src/services/spec_service.rs`
- Modify: `packages/server-rs/src/services/mod.rs`

**Step 1: Create the spec service**

Create `packages/server-rs/src/services/spec_service.rs`. Follow the exact same pattern as `task_service.rs` — functions take `&Connection` as first arg and return `Result<T, AppError>`.

Functions to implement:
- `row_to_spec(row: &Row) -> Result<Spec, rusqlite::Error>` — maps all columns
- `create_spec(conn: &Connection, input: &CreateSpecInput) -> Result<Spec, AppError>` — INSERT with uuid::Uuid::new_v4(), title defaults to first 100 chars of user_input if not provided
- `get_all_specs(conn: &Connection, repository_id: Option<&str>) -> Result<Vec<Spec>, AppError>` — SELECT with optional filter, ORDER BY created_at DESC
- `get_spec_by_id(conn: &Connection, id: &str) -> Result<Spec, AppError>` — SELECT by id, return NotFound if missing
- `update_spec(conn: &Connection, id: &str, input: &UpdateSpecInput) -> Result<Spec, AppError>` — dynamic UPDATE (only set provided fields), always update `updated_at`
- `delete_spec(conn: &Connection, id: &str) -> Result<(), AppError>` — DELETE by id

Use the same column-list constant pattern as task_service:

```rust
const SPEC_COLUMNS: &str = "\
    id, repository_id, title, user_input, draft_spec, final_spec, \
    agent_type, agent_model, status, task_id, created_at, updated_at, approved_at";
```

**Step 2: Register the service module**

In `packages/server-rs/src/services/mod.rs`, add:

```rust
pub mod spec_service;
```

**Step 3: Build to verify**

Run: `cd packages/server-rs && cargo build 2>&1 | tail -5`
Expected: Build succeeds.

**Step 4: Commit**

```bash
git add packages/server-rs/src/services/spec_service.rs packages/server-rs/src/services/mod.rs
git commit -m "feat(server): add spec_service CRUD operations"
```

---

## Task 3: Backend — Spec Agent prompts

**Files:**
- Modify: `packages/server-rs/src/agent/cli_prompts.rs`

**Step 1: Add `build_spec_prompt()`**

Add after the existing `build_implementation_prompt()` function (around line 309):

```rust
/// Builds a prompt for the Spec Agent to explore a codebase and generate
/// a technical specification for a user's idea.
pub fn build_spec_prompt(
    user_input: &str,
    repo_context: Option<&str>,
    workspace_path: Option<&str>,
) -> String {
    let repo_section = repo_context.map(build_repository_section).unwrap_or_default();
    let workspace_section = build_workspace_section(workspace_path);

    format!(
        r#"You are a software design agent. Your job is to explore the codebase and produce a detailed technical specification for the following idea.
{workspace_section}
## User's Idea
{user_input}
{repo_section}
## Your Mission

1. **Explore** the codebase structure, architecture, patterns, and conventions
2. **Analyze** what needs to change to implement the idea
3. **Produce** a technical spec in markdown that includes:
   - Summary of the approach
   - Which files to create, modify, or delete
   - Specific changes needed in each file
   - Design decisions and their rationale
   - Edge cases, risks, and things to watch out for
   - Suggested order of implementation

## Output Format

Write your spec between these delimiters:

---SPEC_START---
(your spec in markdown)
---SPEC_END---

## Rules
- **READ ONLY** — do NOT create, edit, or delete any files
- Do NOT run tests, builds, or commands that modify the filesystem
- Do NOT use git commands
- Your output IS the spec — focus on clarity and completeness
{FORBIDDEN_SECTION}"#
    )
    .trim()
    .to_string()
}

/// Builds a prompt for the Spec Agent to revise a spec based on user feedback.
pub fn build_spec_resume_prompt(
    user_input: &str,
    current_spec: &str,
    feedback: &str,
    repo_context: Option<&str>,
    workspace_path: Option<&str>,
) -> String {
    let repo_section = repo_context.map(build_repository_section).unwrap_or_default();
    let workspace_section = build_workspace_section(workspace_path);

    format!(
        r#"You are a software design agent. You previously generated a spec that the user wants to revise.
{workspace_section}
## Original Idea
{user_input}
{repo_section}
## Current Spec

{current_spec}

## User Feedback

{feedback}

## Your Mission

1. Review the current spec and the user's feedback
2. Explore additional code if needed to address the feedback
3. Produce an updated spec that incorporates the requested changes

Write your updated spec between these delimiters:

---SPEC_START---
(your updated spec in markdown)
---SPEC_END---

## Rules
- **READ ONLY** — do NOT create, edit, or delete any files
- Do NOT run tests, builds, or commands that modify the filesystem
- Do NOT use git commands
{FORBIDDEN_SECTION}"#
    )
    .trim()
    .to_string()
}

/// Builds a prompt for the Spec Agent to continue refining a spec
/// that may have been manually edited by the user.
pub fn build_spec_refine_prompt(
    user_input: &str,
    current_spec: &str,
    repo_context: Option<&str>,
    workspace_path: Option<&str>,
) -> String {
    let repo_section = repo_context.map(build_repository_section).unwrap_or_default();
    let workspace_section = build_workspace_section(workspace_path);

    format!(
        r#"You are a software design agent. The user wants to continue refining a spec (which they may have edited manually).
{workspace_section}
## Original Idea
{user_input}
{repo_section}
## Current Spec (may have been edited by the user)

{current_spec}

## Your Mission

1. Review the current spec and the codebase
2. Suggest improvements, identify gaps, or ask clarifying questions
3. If you have enough context, produce an improved version of the spec

Write your updated spec between these delimiters:

---SPEC_START---
(your updated spec in markdown)
---SPEC_END---

## Rules
- **READ ONLY** — do NOT create, edit, or delete any files
- Do NOT run tests, builds, or commands that modify the filesystem
- Do NOT use git commands
{FORBIDDEN_SECTION}"#
    )
    .trim()
    .to_string()
}
```

**Step 2: Build to verify**

Run: `cd packages/server-rs && cargo build 2>&1 | tail -5`
Expected: Build succeeds.

**Step 3: Commit**

```bash
git add packages/server-rs/src/agent/cli_prompts.rs
git commit -m "feat(server): add Spec Agent prompt builders"
```

---

## Task 4: Backend — Spec routes (CRUD + lifecycle)

**Files:**
- Create: `packages/server-rs/src/routes/specs.rs`
- Modify: `packages/server-rs/src/routes/mod.rs`

**Step 1: Create the specs router**

Create `packages/server-rs/src/routes/specs.rs`. This is the largest backend file. Model it after `routes/tasks.rs` but simpler. It needs:

**CRUD handlers:**
- `create_spec` — POST / — validate input, call spec_service::create_spec, emit data_change
- `list_specs` — GET / — query param `repository_id`, call spec_service::get_all_specs
- `get_spec` — GET /{id} — call spec_service::get_spec_by_id
- `update_spec` — PATCH /{id} — call spec_service::update_spec, emit data_change
- `delete_spec` — DELETE /{id} — cancel agent if running, call spec_service::delete_spec, emit data_change

**Lifecycle handlers:**
- `refine_spec` — POST /{id}/refine — validate status (draft or refining), set status to refining, resolve workspace (reuse `resolve_workspace_base` pattern from tasks.rs — get repo, get repo path, build repo_context), build spec prompt, start agent via agent_service. On agent completion (in the spawned task): parse output for SPEC_START/SPEC_END delimiters, save draft_spec, emit spec_ready SSE event.
- `send_feedback` — POST /{id}/feedback — if agent running: forward via agent_service.send_feedback. If not running: relaunch agent with build_spec_resume_prompt.
- `cancel_spec` — POST /{id}/cancel — cancel agent, set status to canceled
- `approve_spec` — POST /{id}/approve — take final_spec or draft_spec, create Task via task_service::create_task with status "approved", update spec with task_id and approved status
- `spec_logs_stream` — GET /{id}/logs — SSE stream, reuse exact same pattern as task_logs_stream in tasks.rs (subscribe to sse_emitter with spec_id)

**Router:**
```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_spec))
        .route("/", get(list_specs))
        .route("/{id}", get(get_spec))
        .route("/{id}", patch(update_spec))
        .route("/{id}", delete(delete_spec))
        .route("/{id}/refine", post(refine_spec))
        .route("/{id}/feedback", post(send_feedback))
        .route("/{id}/cancel", post(cancel_spec))
        .route("/{id}/approve", post(approve_spec))
        .route("/{id}/logs", get(spec_logs_stream))
}
```

**Key implementation detail for `refine_spec`**: The agent completion handler needs to parse the spec from the output. Add a helper function:

```rust
fn extract_spec_from_output(output: &str) -> Option<String> {
    let start_marker = "---SPEC_START---";
    let end_marker = "---SPEC_END---";
    if let Some(start_idx) = output.find(start_marker) {
        let content_start = start_idx + start_marker.len();
        if let Some(end_idx) = output[content_start..].find(end_marker) {
            let spec = output[content_start..content_start + end_idx].trim();
            if !spec.is_empty() {
                return Some(spec.to_string());
            }
        }
    }
    None
}
```

The agent completion handler in the `tokio::spawn` block for refine should:
1. Get the accumulated output from the runner result summary
2. Try `extract_spec_from_output()`, fallback to full summary
3. Update spec `draft_spec` via spec_service::update_spec
4. Emit `spec_ready` SSE event
5. Keep status as `refining` (not terminal)

**Step 2: Register the route module**

In `packages/server-rs/src/routes/mod.rs`, add:

```rust
pub mod specs;
```

And in the `api_router()` function, add the nest:

```rust
.nest("/api/specs", specs::router())
```

**Step 3: Build to verify**

Run: `cd packages/server-rs && cargo build 2>&1 | tail -5`
Expected: Build succeeds.

**Step 4: Commit**

```bash
git add packages/server-rs/src/routes/specs.rs packages/server-rs/src/routes/mod.rs
git commit -m "feat(server): add spec routes (CRUD + lifecycle)"
```

---

## Task 5: Backend — Build + smoke test

**Files:** None new.

**Step 1: Build release binary**

Run: `cd packages/server-rs && cargo build --release 2>&1 | tail -5`
Expected: Build succeeds.

**Step 2: Copy binary and start server**

```bash
cp packages/server-rs/target/release/agent-board.exe dist/local/agent-board.exe
```

Start the server and verify:
- `curl http://localhost:51767/api/specs` returns `[]`
- `curl -X POST http://localhost:51767/api/specs -H 'Content-Type: application/json' -d '{"repository_id":"test","user_input":"test idea"}'` returns a created spec (or a foreign key error if "test" repo doesn't exist, which is expected)

**Step 3: Commit** (if any fixes were needed)

```bash
git commit -am "fix(server): spec routes fixes from smoke test"
```

---

## Task 6: Frontend — Spec types + API client

**Files:**
- Create: `packages/dashboard/src/features/specs/types/index.ts`
- Modify: `packages/dashboard/src/lib/api-client.ts`

**Step 1: Create spec types**

```typescript
export type SpecStatus = 'draft' | 'refining' | 'approved' | 'failed' | 'canceled'

export interface Spec {
  id: string
  repository_id: string
  title: string
  user_input: string
  draft_spec: string | null
  final_spec: string | null
  agent_type: string | null
  agent_model: string | null
  status: SpecStatus
  task_id: string | null
  created_at: string
  updated_at: string
  approved_at: string | null
}

export interface CreateSpecInput {
  repository_id: string
  user_input: string
  title?: string
  agent_type?: string
  agent_model?: string
}

export interface UpdateSpecInput {
  title?: string
  final_spec?: string | null
}

export const SPEC_STATUS_LABELS: Record<SpecStatus, string> = {
  draft: 'Draft',
  refining: 'Refining',
  approved: 'Approved',
  failed: 'Failed',
  canceled: 'Canceled',
}

export const SPEC_STATUS_COLORS: Record<SpecStatus, string> = {
  draft: 'bg-gray-500/10 text-gray-500',
  refining: 'bg-blue-500/10 text-blue-500',
  approved: 'bg-green-500/10 text-green-500',
  failed: 'bg-red-500/10 text-red-500',
  canceled: 'bg-gray-500/10 text-gray-400',
}
```

**Step 2: Add specsApi to api-client.ts**

In `packages/dashboard/src/lib/api-client.ts`, add after the `tasksApi` object:

```typescript
export const specsApi = {
  getAll: (filters?: { repository_id?: string }) =>
    apiClient.get<Spec[]>('/api/specs', filters),
  getById: (id: string) =>
    apiClient.get<Spec>(`/api/specs/${id}`),
  create: (input: CreateSpecInput) =>
    apiClient.post<Spec>('/api/specs', input),
  update: (id: string, input: UpdateSpecInput) =>
    apiClient.patch<Spec>(`/api/specs/${id}`, input),
  delete: (id: string) =>
    apiClient.delete(`/api/specs/${id}`),
  refine: (id: string) =>
    apiClient.post<{ status: string }>(`/api/specs/${id}/refine`),
  sendFeedback: (id: string, message: string) =>
    apiClient.post<{ status: string }>(`/api/specs/${id}/feedback`, { message }),
  cancel: (id: string) =>
    apiClient.post<{ status: string }>(`/api/specs/${id}/cancel`),
  approve: (id: string) =>
    apiClient.post<{ task: unknown }>(`/api/specs/${id}/approve`),
}
```

Add the import for Spec types at the top of api-client.ts:

```typescript
import type { Spec, CreateSpecInput, UpdateSpecInput } from '@/features/specs/types'
```

**Step 3: Commit**

```bash
git add packages/dashboard/src/features/specs/types/index.ts packages/dashboard/src/lib/api-client.ts
git commit -m "feat(dashboard): add spec types and API client"
```

---

## Task 7: Frontend — Spec hooks (query + mutations)

**Files:**
- Create: `packages/dashboard/src/features/specs/hooks/query-keys.ts`
- Create: `packages/dashboard/src/features/specs/hooks/use-specs.ts`
- Create: `packages/dashboard/src/features/specs/hooks/use-spec.ts`
- Create: `packages/dashboard/src/features/specs/hooks/use-spec-mutations.ts`
- Create: `packages/dashboard/src/features/specs/hooks/use-spec-actions.ts`

**Step 1: Create query keys**

`query-keys.ts`:
```typescript
export const specKeys = {
  all: ['specs'] as const,
  lists: () => [...specKeys.all, 'list'] as const,
  list: (filters: { repository_id?: string }) => [...specKeys.lists(), filters] as const,
  details: () => [...specKeys.all, 'detail'] as const,
  detail: (id: string) => [...specKeys.details(), id] as const,
}
```

**Step 2: Create use-specs hook**

`use-specs.ts`:
```typescript
import { useQuery } from '@tanstack/react-query'
import { specKeys } from './query-keys'
import { specsApi } from '@/lib/api-client'

export function useSpecs(filters: { repository_id?: string } = {}) {
  return useQuery({
    queryKey: specKeys.list(filters),
    queryFn: () => specsApi.getAll(filters),
    staleTime: 30_000,
    refetchInterval: (query) => {
      const specs = query.state.data
      if (specs?.some(s => s.status === 'refining')) return 3000
      return false
    },
  })
}
```

**Step 3: Create use-spec hook**

`use-spec.ts`:
```typescript
import { useQuery } from '@tanstack/react-query'
import { specKeys } from './query-keys'
import { specsApi } from '@/lib/api-client'

export function useSpec(id: string | null) {
  return useQuery({
    queryKey: specKeys.detail(id!),
    queryFn: () => specsApi.getById(id!),
    enabled: !!id,
  })
}
```

**Step 4: Create use-spec-mutations hook**

`use-spec-mutations.ts`:
```typescript
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { specsApi } from '@/lib/api-client'
import { specKeys } from './query-keys'
import type { CreateSpecInput, UpdateSpecInput } from '../types'

export function useCreateSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateSpecInput) => specsApi.create(input),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}

export function useUpdateSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateSpecInput }) =>
      specsApi.update(id, input),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}

export function useDeleteSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => specsApi.delete(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}
```

**Step 5: Create use-spec-actions hook**

`use-spec-actions.ts`:
```typescript
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { specsApi } from '@/lib/api-client'
import { specKeys } from './query-keys'

export function useRefineSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => specsApi.refine(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}

export function useSpecFeedback() {
  return useMutation({
    mutationFn: ({ id, message }: { id: string; message: string }) =>
      specsApi.sendFeedback(id, message),
  })
}

export function useCancelSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => specsApi.cancel(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}

export function useApproveSpec() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => specsApi.approve(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: specKeys.all }),
  })
}
```

**Step 6: Commit**

```bash
git add packages/dashboard/src/features/specs/hooks/
git commit -m "feat(dashboard): add spec query hooks and mutations"
```

---

## Task 8: Frontend — Spec SSE hook

**Files:**
- Create: `packages/dashboard/src/features/specs/hooks/use-spec-sse.ts`

**Step 1: Create SSE hook**

Model after `use-task-sse.ts` but simplified. The hook connects to `GET /api/specs/:id/logs` and handles:
- `log` events → append to logs array
- `status` events → invalidate spec query
- `chat_message` events → append to chat messages
- `spec_ready` events → invalidate spec query (draft_spec updated)
- `error` events → surface error
- `tool_activity` events → surface tool activity (for the collapsible logs)

The hook should use the same `EventSource` pattern as `use-task-sse.ts` — create EventSource with auth token as query param, handle reconnection. Keep it simpler: no connection manager class, just a `useEffect` with cleanup.

Returns: `{ logs, chatMessages, connectionStatus, isAgentRunning }`

**Step 2: Commit**

```bash
git add packages/dashboard/src/features/specs/hooks/use-spec-sse.ts
git commit -m "feat(dashboard): add spec SSE hook for chat and logs"
```

---

## Task 9: Frontend — Spec UI store + status badge

**Files:**
- Create: `packages/dashboard/src/features/specs/stores/spec-ui-store.ts`
- Create: `packages/dashboard/src/features/specs/components/spec-status-badge.tsx`

**Step 1: Create UI store**

`spec-ui-store.ts` — Zustand store with persist:
```typescript
interface SpecUIState {
  statusFilter: string[]
  setStatusFilter: (statuses: string[]) => void
  drawerSpecId: string | null
  openDrawer: (specId: string) => void
  closeDrawer: () => void
  isCreateModalOpen: boolean
  openCreateModal: () => void
  closeCreateModal: () => void
}
```

**Step 2: Create status badge**

`spec-status-badge.tsx` — reuse the same Badge component from shadcn/ui, apply colors from `SPEC_STATUS_COLORS`.

**Step 3: Commit**

```bash
git add packages/dashboard/src/features/specs/stores/ packages/dashboard/src/features/specs/components/spec-status-badge.tsx
git commit -m "feat(dashboard): add spec UI store and status badge"
```

---

## Task 10: Frontend — Create Spec dialog

**Files:**
- Create: `packages/dashboard/src/features/specs/components/create-spec-dialog.tsx`

**Step 1: Create the dialog**

Model after `create-task-dialog.tsx`. Fields:
- Textarea for user idea (user_input)
- Agent type selector (reuse `useDetectedAgents` hook)
- Agent model selector
- Repository comes from `useRepoStore().selectedRepoId`

On submit: call `useCreateSpec()`, then navigate to `/specs` and open drawer with the new spec ID, then auto-trigger refine.

**Step 2: Commit**

```bash
git add packages/dashboard/src/features/specs/components/create-spec-dialog.tsx
git commit -m "feat(dashboard): add create spec dialog"
```

---

## Task 11: Frontend — Spec list + list item

**Files:**
- Create: `packages/dashboard/src/features/specs/components/spec-list.tsx`
- Create: `packages/dashboard/src/features/specs/components/spec-list-item.tsx`

**Step 1: Create spec list**

`spec-list.tsx` — uses `useSpecs()` with `repository_id` from `useRepoStore`. Shows header with "Specs" title + "+ New Spec" button. Status filter buttons. Maps specs to SpecListItem components.

**Step 2: Create spec list item**

`spec-list-item.tsx` — card showing: title, SpecStatusBadge, repo name (if available), relative date, truncated user_input preview. onClick → opens drawer via `specUIStore.openDrawer(spec.id)`.

**Step 3: Commit**

```bash
git add packages/dashboard/src/features/specs/components/spec-list.tsx packages/dashboard/src/features/specs/components/spec-list-item.tsx
git commit -m "feat(dashboard): add spec list and list item components"
```

---

## Task 12: Frontend — Spec chat component

**Files:**
- Create: `packages/dashboard/src/features/specs/components/spec-chat.tsx`

**Step 1: Create chat component**

`spec-chat.tsx` — the chat tab inside the drawer. Contains:
- Scrollable message area (chat messages from SSE)
- Each message shows role (user/agent), content, timestamp
- Input textarea at the bottom with send button
- Activity indicator when agent is running (from SSE connection status)
- Collapsible logs section (collapsed by default) — renders log entries from SSE
- "View Spec →" button that appears when `draft_spec` is populated and agent is not running — navigates to `/specs/:specId`

Uses `useSpecSSE()` for real-time updates, `useSpecFeedback()` for sending messages.

**Step 2: Commit**

```bash
git add packages/dashboard/src/features/specs/components/spec-chat.tsx
git commit -m "feat(dashboard): add spec chat component"
```

---

## Task 13: Frontend — Spec drawer

**Files:**
- Create: `packages/dashboard/src/features/specs/components/spec-drawer.tsx`

**Step 1: Create drawer**

`spec-drawer.tsx` — uses Sheet from shadcn/ui (same as TaskDrawer). Opens from right side. Contains:
- Header: spec title, status badge, close button
- Tab bar: "Chat" | "Spec" (using shadcn Tabs component)
- Tab Chat → renders `<SpecChat />`
- Tab Spec → renders a read-only preview of draft_spec/final_spec (simple markdown render)
- Controlled by `specUIStore.drawerSpecId`

Uses `useSpec(drawerSpecId)` to fetch spec data.

**Step 2: Commit**

```bash
git add packages/dashboard/src/features/specs/components/spec-drawer.tsx
git commit -m "feat(dashboard): add spec drawer with chat and spec tabs"
```

---

## Task 14: Frontend — Spec detail page (editor + actions)

**Files:**
- Create: `packages/dashboard/src/features/specs/components/spec-detail.tsx`
- Create: `packages/dashboard/src/features/specs/components/spec-editor.tsx`
- Create: `packages/dashboard/src/app/specs/[specId]/page.tsx`

**Step 1: Create spec editor**

`spec-editor.tsx` — markdown editor with edit/preview toggle:
- Edit mode: textarea with monospace font, full height
- Preview mode: rendered markdown (use a simple markdown-to-html approach or dangerouslySetInnerHTML with basic sanitization — or a lightweight md renderer if one is already in the project)
- "Save" button that calls `useUpdateSpec()` with `{ final_spec: editedText }`

**Step 2: Create spec detail**

`spec-detail.tsx` — the dedicated page layout:
- Header: editable title (inline-edit pattern from tasks), status badge, repo name
- Body: `<SpecEditor />` component
- Footer/action bar:
  - "Continue Refining" button → navigates to `/specs`, opens drawer, triggers refine with current spec
  - "Send to Code" button → calls `useApproveSpec()`, shows toast with link to created task

**Step 3: Create page route file**

`packages/dashboard/src/app/specs/[specId]/page.tsx`:
```typescript
import { SpecDetail } from '@/features/specs/components/spec-detail'

export default function SpecDetailPage() {
  return <SpecDetail />
}
```

**Step 4: Commit**

```bash
git add packages/dashboard/src/features/specs/components/spec-detail.tsx packages/dashboard/src/features/specs/components/spec-editor.tsx packages/dashboard/src/app/specs/\[specId\]/page.tsx
git commit -m "feat(dashboard): add spec detail page with editor and actions"
```

---

## Task 15: Frontend — Specs page + routing + sidebar

**Files:**
- Create: `packages/dashboard/src/app/specs/page.tsx`
- Modify: `packages/dashboard/src/router.tsx`
- Modify: `packages/dashboard/src/components/layout/sidebar.tsx`
- Create: `packages/dashboard/src/features/specs/index.ts` (barrel export)

**Step 1: Create specs page**

`packages/dashboard/src/app/specs/page.tsx`:
```typescript
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
```

**Step 2: Create barrel export**

`packages/dashboard/src/features/specs/index.ts`:
```typescript
export { SpecList } from './components/spec-list'
export { SpecDrawer } from './components/spec-drawer'
export { CreateSpecDialog } from './components/create-spec-dialog'
export { SpecDetail } from './components/spec-detail'
```

**Step 3: Add routes to router.tsx**

Import the pages:
```typescript
import SpecsPage from '@/app/specs/page'
import SpecDetailPage from '@/app/specs/[specId]/page'
```

Add the routes:
```typescript
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
```

Add to route tree inside mainLayoutRoute.addChildren:
```typescript
mainLayoutRoute.addChildren([
  boardRoute,
  specsRoute,      // ← NEW
  specDetailRoute,  // ← NEW
  diffRoute,
  settingsRoute,
]),
```

**Step 4: Add sidebar nav item**

In `packages/dashboard/src/components/layout/sidebar.tsx`, add `FileText` to lucide imports:
```typescript
import { ..., FileText } from 'lucide-react'
```

Add to navItems array between Board and Tasks:
```typescript
{
  title: 'Specs',
  href: '/specs',
  icon: FileText,
},
```

**Step 5: Build to verify**

Run: `cd packages/dashboard && npm run build 2>&1 | tail -10`
Expected: Build succeeds.

**Step 6: Commit**

```bash
git add packages/dashboard/src/app/specs/ packages/dashboard/src/features/specs/ packages/dashboard/src/router.tsx packages/dashboard/src/components/layout/sidebar.tsx
git commit -m "feat(dashboard): add specs page, routing, and sidebar nav"
```

---

## Task 16: Frontend — Data invalidation for specs

**Files:**
- Modify: `packages/dashboard/src/components/shared/providers.tsx` (or wherever `useDataInvalidation` is configured)

**Step 1: Add spec cache invalidation**

Find where `useDataInvalidation` handles `data-change` SSE events. Add a case for `entity === 'spec'` that invalidates `specKeys.all`.

This ensures that when the backend emits `data_emitter.emit_change("spec", "updated", ...)`, the frontend specs list and detail queries refresh automatically.

**Step 2: Build and verify**

Run: `cd packages/dashboard && npm run build 2>&1 | tail -10`
Expected: Build succeeds.

**Step 3: Commit**

```bash
git add packages/dashboard/src/components/shared/
git commit -m "feat(dashboard): add spec data invalidation via SSE"
```

---

## Task 17: Integration test — full flow

**Files:** None new.

**Step 1: Build everything**

```bash
cd packages/shared && npm run build
cd ../dashboard && npm run build
cd ../server-rs && cargo build --release
```

**Step 2: Copy binary and start**

```bash
cp packages/server-rs/target/release/agent-board.exe dist/local/agent-board.exe
```

Start the server.

**Step 3: Manual test flow**

1. Open browser, go to the dashboard
2. Verify "Specs" appears in sidebar
3. Click Specs → verify empty list
4. Click "+ New Spec" → fill in idea, select agent
5. Verify spec is created and drawer opens
6. Verify agent starts (SSE logs appear)
7. Wait for agent to produce draft spec
8. Send feedback in chat
9. Click "View Spec →" → verify dedicated page loads
10. Edit spec markdown, save
11. Click "Send to Code" → verify task is created
12. Go to Board → verify task appears as "approved"

**Step 4: Final commit** (if any fixes)

```bash
git commit -am "fix: spec agent integration fixes"
```
