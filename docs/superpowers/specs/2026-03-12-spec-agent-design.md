# Spec Agent Feature Design

## Overview

Add a **Spec Agent** feature to agent-board that separates spec generation from code execution. Users create specs via an AI agent that explores the codebase and collaborates through chat, then create tasks from finalized specs. This introduces a new `Spec` entity, a dedicated `/specs` board UI, and a specialized Spec Agent prompt.

## Goals

- Decouple spec generation from task execution into two distinct workflows
- Allow the Spec Agent to explore the codebase (read-only) and ask the user clarifying questions via chat
- Produce structured, consistent specs using a fixed Markdown template
- Enable users to review and edit specs before creating tasks
- Maintain traceability between Specs and Tasks (1:1 relationship)

## Non-Goals

- RAG / institutional memory across specs
- Multiple tasks per spec
- Drag & drop in specs board
- User-configurable spec templates
- Spec Agent writing or modifying code

---

## Data Model

### New Table: `specs` (Migration 12)

```sql
CREATE TABLE specs (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    user_input TEXT NOT NULL,
    repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'draft',
    spec_content TEXT,
    agent_type TEXT,
    agent_model TEXT,
    task_id TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_specs_repo ON specs(repository_id);
CREATE INDEX idx_specs_status ON specs(status);
```

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | TEXT (UUID) | Primary key |
| `title` | TEXT | Spec title (user-provided or auto-generated from user_input) |
| `user_input` | TEXT | Raw user request describing what they want |
| `repository_id` | TEXT (FK) | Repository this spec is for |
| `status` | TEXT | `draft` \| `generating` \| `ready` \| `failed` \| `cancelled` |
| `spec_content` | TEXT | Generated Markdown spec (null until agent completes) |
| `agent_type` | TEXT | CLI agent used for generation (e.g., `claude-code`) |
| `agent_model` | TEXT | Model used (e.g., `claude-sonnet-4-6`) |
| `task_id` | TEXT | FK to task created via "Create Task" (null until then) |
| `error` | TEXT | Error message if agent failed |

### Modified Table: `tasks`

```sql
ALTER TABLE tasks ADD COLUMN spec_id TEXT REFERENCES specs(id) ON DELETE SET NULL;
```

Enables "View Spec" link from task detail. `ON DELETE SET NULL` ensures deleting a spec leaves the task intact with `spec_id = NULL` (UI shows "Spec deleted").

### Rename: `task_logs` → `event_logs`

```sql
ALTER TABLE task_logs RENAME TO event_logs;
ALTER TABLE event_logs RENAME COLUMN task_id TO entity_id;
ALTER TABLE event_logs ADD COLUMN entity_type TEXT NOT NULL DEFAULT 'task';
```

**Important notes on this migration:**
- The FK constraint on the old `task_id` column (`REFERENCES tasks(id) ON DELETE CASCADE`) must be dropped. Since SQLite doesn't support `ALTER TABLE DROP CONSTRAINT`, the migration must recreate the table without the FK. The `entity_id` column becomes a plain TEXT field — referential integrity is enforced at the application layer.
- Existing rows get `entity_type = 'task'`. Spec events use `entity_type = 'spec'`.
- **Scope of code changes**: This rename affects ~30-40 references across the codebase, primarily in `routes/data.rs` (~25 refs including struct field names, SQL queries, export/import logic), `services/task_event_service.rs` (`PersistEvent.task_id` → `entity_id`), and `utils/sse_emitter.rs` (`PersistMsg.task_id` → `entity_id`). The export/import API contract changes (JSON field name `task_logs` → `event_logs`).
- **Breaking change for export/import**: Existing exported JSON files use the key `task_logs`. After the rename, import should accept both `task_logs` (legacy) and `event_logs` (new) as aliases for backward compatibility.

### Spec Status Lifecycle

```
draft → generating → ready
  ↑         |
  |         v
  +------ failed

draft → generating → cancelled (user-initiated)
```

- `draft`: Created, agent not started
- `generating`: Spec Agent running, can chat with user
- `ready`: Spec complete, user can view/edit/create-task
- `failed`: Agent crashed or timed out (retryable)
- `cancelled`: User explicitly cancelled (retryable, no error message shown)

### Spec Content Template

The Spec Agent must generate Markdown following this structure:

```markdown
## Objetivo
[What the feature/change aims to accomplish]

## Cambios propuestos
[Detailed description of what needs to change]

## Archivos a modificar
[List of files that need changes, with brief description of each change]

## Criterios de aceptación
[Measurable criteria to verify the implementation is correct]
```

---

## Backend API

### New Endpoints

| Method | Path | Description | Validations |
|--------|------|-------------|-------------|
| `POST /api/specs` | Create spec | `user_input` and `repository_id` required |
| `GET /api/specs` | List specs | Optional filters: `repository_id`, `status` |
| `GET /api/specs/:id` | Get spec by ID | — |
| `PATCH /api/specs/:id` | Edit spec | Only if status = `ready` or `draft` |
| `DELETE /api/specs/:id` | Delete spec | Any status (cancels agent if `generating`) |
| `POST /api/specs/:id/generate` | Start Spec Agent | Only if status = `draft`, `failed`, or `cancelled` |
| `POST /api/specs/:id/feedback` | Send chat message to agent | Only if status = `generating` |
| `POST /api/specs/:id/cancel` | Cancel running agent | Only if status = `generating` → sets `cancelled` |
| `POST /api/specs/:id/create-task` | Create Task from spec | Only if status = `ready` and `task_id` = NULL |
| `GET /api/specs/:id/logs` | SSE event stream | Same pattern as task logs |

### Generate Flow (`POST /api/specs/:id/generate`)

1. Validate status = `draft`, `failed`, or `cancelled`
2. Update status → `generating`, clear error if retry
3. Spawn background task:
   a. `resolve_workspace()` — create temporary git worktree (read-only)
   b. Build `repo_context` from repository metadata (stack, conventions, patterns)
   c. Build prompt with `cli_prompts::build_spec_prompt()`
   d. Start CLI agent via `CLIAgentRunner` with spec ID for SSE routing
4. Agent runs, can ask user questions via chat (feedback writes to stdin)
5. On success (exit code 0):
   - Parse output for spec content (find Markdown with template sections)
   - Update spec: `spec_content` = parsed content, status → `ready`
   - Cleanup temporary worktree
   - Emit SSE `complete` event
6. On failure:
   - Update spec: status → `failed`, error = message
   - Cleanup temporary worktree
   - Emit SSE `error` event

### Create Task Flow (`POST /api/specs/:id/create-task`)

1. Validate status = `ready` and `task_id` = NULL
2. Create new Task:
   - `title` = spec.title
   - `description` = spec.spec_content (copy)
   - `repository_id` = spec.repository_id
   - `status` = `draft`
   - `spec_id` = spec.id
3. Update spec: `task_id` = new task ID
4. Emit data-change events for both spec and task
5. Return the created Task

### Feedback Flow (`POST /api/specs/:id/feedback`)

Same mechanism as task feedback:
1. If agent running → write message to process stdin
2. If agent not running → resume agent with feedback as additional context
3. Store chat message in SSE event history

---

## AgentService Adaptation

### Problem

`AgentService.start_agent()` currently has task-specific completion logic hardcoded:
- On success: calls `task_service::update_task()`, sets `TaskStatus::AwaitingReview`, extracts git changes via `extract_changes_data()`
- On failure: calls `task_service::update_task()` with `TaskStatus::Failed`
- Emits data-change events with `entity: "task"`

For specs, the completion logic is different:
- On success: calls `spec_service::update_spec()`, sets `SpecStatus::Ready`, parses output for `spec_content` (no git changes extraction)
- On failure: calls `spec_service::update_spec()` with `SpecStatus::Failed`
- Emits data-change events with `entity: "spec"`

### Solution: Entity Context Enum

Introduce an `AgentEntityContext` enum passed alongside `CLIRunnerOptions`:

```rust
pub enum AgentEntityContext {
    Task {
        task_id: String,
    },
    Spec {
        spec_id: String,
    },
}
```

The completion callback in `start_agent()` dispatches on this enum:

```rust
match entity_context {
    AgentEntityContext::Task { task_id } => {
        // Existing task completion logic (extract changes, update task, etc.)
    },
    AgentEntityContext::Spec { spec_id } => {
        // Parse spec_content from agent output
        // Update spec status → ready
        // Cleanup worktree
        // Emit data-change("spec", ...)
    },
}
```

This keeps the existing task flow untouched while adding spec support in the same method. The `CLIRunnerOptions.task_id` field is renamed to `entity_id` for clarity (used for SSE channel routing).

### Spec-Specific Completion Logic

On agent success (exit code 0):
1. Extract the last substantial assistant message from the output (same parsing as task summary extraction)
2. Look for Markdown with template sections (`## Objetivo`, etc.)
3. If found: save as `spec_content`
4. If not found: save the full last assistant message as `spec_content` (user can edit)
5. Update spec: status → `ready`
6. Cleanup temporary worktree
7. Emit SSE `complete` event

On agent failure:
1. Update spec: status → `failed`, error = message
2. Cleanup temporary worktree
3. Emit SSE `error` event

**No `extract_changes_data()` call** — the Spec Agent is read-only, there are no git changes to capture.

---

## Spec Agent Prompt

New function: `cli_prompts::build_spec_prompt()`

**Parameters:** `title`, `user_input`, `context_files`, `repo_context`, `agent_type`, `workspace_path`

**Prompt structure:**

- **Role**: "You are a Spec Agent. Your job is to generate a technical specification, NOT to write code."
- **Workspace boundary**: Same as task prompts (restrict to workspace path)
- **User request**: The title and user_input
- **Repository context**: Stack, conventions, learned patterns
- **Capabilities**: "You can explore the codebase using Read, Grep, Glob, and Bash (read-only commands only) to understand the structure and inform your spec."
- **Interaction**: "You may ask the user clarifying questions via chat to refine requirements. Ask one question at a time."
- **Output format**: "Generate a Markdown document with these exact sections: ## Objetivo, ## Cambios propuestos, ## Archivos a modificar, ## Criterios de aceptación"
- **Forbidden**: "Do NOT create, edit, or modify any files. Do NOT run builds, tests, or dev servers. Do NOT execute git commands."
- **Agent-specific instructions**: Same per-agent adjustments as task prompts (e.g., Codex gets "direct execution" instruction)

### Tool Restrictions

The prompt instructs the agent to only use read-only tools. The CLI agent itself has full tool access (we can't restrict at CLI level for all agents), but the prompt clearly forbids write operations.

For Claude Code specifically, `--allowedTools Read,Bash,Grep,Glob` restricts at the CLI level.

---

## SSE Integration

### Event Types (same as tasks)

| Event | Usage in Specs |
|-------|---------------|
| `log` | Agent info/warn/error messages |
| `status` | Status changes (draft → generating → ready) |
| `chat_message` | Agent ↔ user conversation |
| `tool_activity` | Agent reading files, searching code |
| `complete` | Spec generated successfully |
| `error` | Agent failure |

### SSE Architecture

- Reuses existing `SSEEmitter` — spec IDs used as channel keys (no collision with task IDs, both are UUIDs)
- Endpoint: `GET /api/specs/:id/logs` with `?token=` auth
- History replay: up to 1500 events per spec
- Persistence: events stored in `event_logs` table with `entity_type = 'spec'`

---

## Frontend

### New Files

```
packages/dashboard/src/
├── app/specs/
│   ├── page.tsx                     # SpecsPage (board wrapper)
│   └── [specId]/
│       └── page.tsx                 # SpecDetailPage (view/edit spec content)
├── features/specs/
│   ├── components/
│   │   ├── spec-board-view.tsx      # Kanban board (3 columns)
│   │   ├── spec-board-column.tsx    # Single column
│   │   ├── spec-board-card.tsx      # Spec card in column
│   │   ├── spec-drawer.tsx          # Right-side drawer (chat + actions)
│   │   ├── spec-chat.tsx            # Chat component (mirrors TaskChat)
│   │   ├── create-spec-dialog.tsx   # Create spec dialog
│   │   └── index.ts
│   ├── hooks/
│   │   ├── query-keys.ts           # specKeys query key factory
│   │   ├── use-specs.ts            # Query all specs
│   │   ├── use-spec.ts             # Query single spec by ID
│   │   ├── use-spec-actions.ts     # Mutations (generate, cancel, create-task)
│   │   ├── use-spec-chat.ts        # Chat hook (same pattern as useTaskChat)
│   │   ├── use-spec-sse.ts         # SSE connection (same pattern as useTaskSSE)
│   │   └── index.ts
│   ├── stores/
│   │   └── spec-ui-store.ts        # Zustand: drawerSpecId, isCreateModalOpen
│   └── index.ts
```

### Modified Files

| File | Change |
|------|--------|
| `components/layout/sidebar.tsx` | Add "Specs" nav item with `FileText` icon, between Board and Tasks |
| `router.tsx` | Add `specsRoute` (`/specs`) and `specDetailRoute` (`/specs/$specId`) under `mainLayoutRoute` |
| `lib/api-client.ts` | Add `specsApi` object with all spec endpoints |
| `features/tasks/components/task-detail.tsx` | Add "View Spec" link if `task.spec_id` exists |
| `hooks/use-data-invalidation.ts` | Handle `entity === 'spec'` to invalidate `specKeys` query keys |

### Specs Board (3 Columns)

Same visual design as the existing task board (colors, card style, layout):

**Note:** Routes are registered in `router.tsx` via `createRoute()` (TanStack Router, code-based). The `app/` directory is a convention for page components, not file-based routing. `$specId` follows TanStack Router param naming convention.

| Column | Statuses | Color | Header Action |
|--------|----------|-------|---------------|
| Draft | `draft`, `failed`, `cancelled` | Gray (same as tasks "To Do") | `+` button to create |
| Generating | `generating` | Amber/yellow | — |
| Ready | `ready` | Green | — |

Cards show: title, repo name, relative time, status indicators (spinner for generating, error icon for failed).

### Spec Drawer (right-side sheet)

Mirrors `TaskDrawer` layout:

**Status `draft`:**
- Editable title (inline edit)
- User input description
- Agent/model selector (`AgentModelSelector` reused)
- "Generate Spec" button

**Status `generating`:**
- Animated status badge
- SpecChat component (chat + tool activity)
- "Cancel" button

**Status `ready`:**
- "View Spec" button → navigates to `/specs/:id`
- "Create Task" button (disabled if `task_id` exists)
- If task exists: "View Task →" link
- Chat in read-only mode (conversation history)

**Status `failed`:**
- Error message display
- "Retry" button (re-triggers generate)

### Spec Detail Page (`/specs/:id`)

Full-page view for reviewing and editing spec content:
- Rendered Markdown view of `spec_content`
- "Edit" button → switches to textarea editor
- "Save" button → `PATCH /api/specs/:id`
- "Create Task" button (same as drawer)
- Breadcrumb navigation: Specs → {title}

### Chat Component Reuse Strategy

Create `SpecChat` as a new component that mirrors `TaskChat` structure but uses `useSpecChat` + `specsApi.feedback()`. This avoids modifying the existing task chat component.

Shared sub-components (`ChatMessageBubble`, `ToolBadge`) are extracted to `components/shared/chat/` and imported by both `TaskChat` and `SpecChat`.

### Sidebar Navigation Order

```
Board      → /board
Specs      → /specs      (NEW)
Tasks      → /tasks
─────────────────────
Settings   → /settings
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Spec Agent crashes/times out | Status → `failed`, error saved, "Retry" button shown |
| User cancels during `generating` | Agent killed, status → `cancelled` (no error message shown, "Retry" button available) |
| User edits spec in `ready` | `PATCH` updates `spec_content`, stays in `ready` |
| User deletes spec that has a task | Task becomes orphaned (`spec_id` points to deleted spec). "View Spec" link in task shows "Spec deleted". Task is NOT cascade-deleted. |
| User deletes spec in `generating` | Agent cancelled first, then spec deleted |
| Spec Agent doesn't follow template | Backend saves whatever was returned as `spec_content`. User can edit manually. |
| "Create Task" fails | Spec stays in `ready` without `task_id`, user can retry |
| Repository deleted | CASCADE delete — all specs for that repo are deleted |

---

## Worktree Management

- **Spec Agent**: Creates a temporary worktree for read-only codebase exploration. Cleaned up after agent completes (success or failure).
- **Coding Agent (Task)**: Creates its own independent worktree when the task is started. No relationship to the spec's worktree.
- Rationale: Spec Agent only reads — nothing to reuse. Time may pass between spec completion and task start, so a fresh worktree ensures up-to-date code.

---

## Shared Package

### New File: `packages/shared/src/schemas/spec.schema.ts`

Following the established pattern (per MEMORY.md: "Add to `schemas/*.schema.ts`, export in `schemas/index.ts` AND `src/index.ts`"):

- `SpecStatus` enum: `draft | generating | ready | failed | cancelled`
- `SpecSchema`: Zod schema for the Spec entity
- `CreateSpecInputSchema`: Zod schema for creation input
- `UpdateSpecInputSchema`: Zod schema for update input

Export in `schemas/index.ts` and `src/index.ts`.

---

## New Rust Files

| File | Purpose | Estimated Lines |
|------|---------|----------------|
| `models/spec.rs` | Spec struct, SpecStatus enum, CreateSpecInput, UpdateSpecInput | ~120 |
| `services/spec_service.rs` | CRUD operations, status validation, row mapping | ~300 |
| `routes/specs.rs` | HTTP handlers for all 10 endpoints | ~600 |
| `agent/cli_prompts.rs` (modified) | Add `build_spec_prompt()` function | +60 |
| `db/migrations.rs` (modified) | Migration 12: specs table, task.spec_id, event_logs rename | +40 |
| `services/agent_service.rs` (modified) | Add `AgentEntityContext` enum, dispatch on entity type in completion | +80 |
| `agent/types.rs` (modified) | Rename `task_id` → `entity_id` in `CLIRunnerOptions` | +5 |
| `routes/mod.rs` (modified) | Register spec routes | +5 |
| `models/mod.rs` (modified) | Export spec module | +2 |
| `services/mod.rs` (modified) | Export spec_service | +2 |
| `lib.rs` (modified) | Add SpecService to AppState (if needed) | +5 |
| `routes/data.rs` (modified) | Rename `task_logs` → `event_logs` (~25 refs: structs, SQL, export/import) | ~40 |
| `services/task_event_service.rs` (modified) | Rename table + column refs | ~10 |
| `utils/sse_emitter.rs` (modified) | Rename table refs in persistence | ~5 |

**Total estimated: ~1,350 new/modified lines (Rust)**

### New Frontend Files

| File | Purpose | Estimated Lines |
|------|---------|----------------|
| `app/specs/page.tsx` | Specs page wrapper | ~30 |
| `app/specs/[specId]/page.tsx` | Spec detail/editor page | ~150 |
| `features/specs/components/spec-board-view.tsx` | Kanban board | ~120 |
| `features/specs/components/spec-board-column.tsx` | Column component | ~80 |
| `features/specs/components/spec-board-card.tsx` | Card component | ~90 |
| `features/specs/components/spec-drawer.tsx` | Drawer with actions | ~200 |
| `features/specs/components/spec-chat.tsx` | Chat component | ~150 |
| `features/specs/components/create-spec-dialog.tsx` | Create dialog | ~180 |
| `features/specs/hooks/query-keys.ts` | Query key factory | ~15 |
| `features/specs/hooks/use-specs.ts` | List query | ~30 |
| `features/specs/hooks/use-spec.ts` | Single query | ~20 |
| `features/specs/hooks/use-spec-actions.ts` | Mutations | ~80 |
| `features/specs/hooks/use-spec-chat.ts` | Chat state | ~100 |
| `features/specs/hooks/use-spec-sse.ts` | SSE connection | ~60 |
| `features/specs/stores/spec-ui-store.ts` | Zustand store | ~40 |
| `components/shared/chat/` | Extracted ChatMessageBubble, ToolBadge | ~150 |

**Total estimated: ~1,495 new lines (Frontend)**

| `packages/shared/src/schemas/spec.schema.ts` | Zod schemas + types | ~60 |

**Total estimated: ~1,555 new lines (Frontend + Shared)**

**Grand total: ~2,905 lines across backend, frontend, and shared**
