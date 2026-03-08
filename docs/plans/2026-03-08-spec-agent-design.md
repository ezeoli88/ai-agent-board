# Spec Agent Feature Design

## Overview

Add a "Spec Agent" feature to agent-board that enables collaborative spec generation before coding. A dedicated agent reads the repository, produces a technical spec draft, and engages in back-and-forth conversation with the user until the spec is approved. Once approved, the spec creates a Task in "approved" status for the Dev Agent to execute.

## Decisions

- **Agent**: Same CLI agent (e.g., Claude Code) with a spec-specific prompt, running in plan-only mode (read-only tools)
- **Flow**: Hybrid — agent reads repo and generates draft, then enters chat mode for iterative refinement
- **Approval outcome**: Creates Task with status "approved" (user decides when to execute Dev Agent)
- **Entity model**: Spec is a new table, separate from tasks, sharing agent_service and SSE infrastructure
- **UI**: `/specs` page with list + drawer (chat), `/specs/:specId` dedicated page (editor + actions)
- **Spec editing**: User can edit markdown directly in dedicated page, plus chat-based refinement
- **Spec ↔ Repo**: 1:1 relationship, each spec belongs to a repository
- **Feedback level**: Chat + collapsible logs (collapsed by default)

## Data Model

### Table: `specs`

```sql
CREATE TABLE IF NOT EXISTS specs (
    id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL,

    -- Content
    title TEXT NOT NULL,
    user_input TEXT NOT NULL,
    draft_spec TEXT,
    final_spec TEXT,

    -- Agent config
    agent_type TEXT,
    agent_model TEXT,

    -- Lifecycle
    status TEXT DEFAULT 'draft',  -- draft | refining | approved | failed | canceled
    task_id TEXT,                  -- FK to task created on approval

    -- Timestamps
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
    approved_at TEXT,

    FOREIGN KEY (repository_id) REFERENCES repositories(id),
    FOREIGN KEY (task_id) REFERENCES tasks(id)
);

CREATE INDEX IF NOT EXISTS idx_specs_repository_id ON specs(repository_id);
CREATE INDEX IF NOT EXISTS idx_specs_status ON specs(status);
```

### Lifecycle

```
draft → refining → approved
  ↓        ↓
failed   canceled
```

### Chat History

Reuses `task_logs` table and `SSEEmitter` with `spec_id` as the entity key. No schema change needed — `task_logs.task_id` is TEXT without FK constraint at DB level.

## Backend

### API Endpoints

```
POST   /api/specs              — Create spec (repository_id, user_input, agent_type, agent_model)
GET    /api/specs              — List specs (filter: ?repository_id=)
GET    /api/specs/:id          — Get spec by ID
PATCH  /api/specs/:id          — Update (title, final_spec)
DELETE /api/specs/:id          — Delete spec
POST   /api/specs/:id/refine   — Launch Spec Agent (reads repo, generates draft)
POST   /api/specs/:id/feedback — Send chat message to agent
POST   /api/specs/:id/cancel   — Cancel agent
POST   /api/specs/:id/approve  — Approve spec → creates Task with status "approved"
GET    /api/specs/:id/logs     — SSE stream (chat + logs)
```

### Spec Agent Prompts

Three prompt builders in `cli_prompts.rs`:

**`build_spec_prompt()`** — Initial generation:
- Instructs agent to explore codebase and produce a technical spec
- Plan-only mode (Read, Bash read-only, Grep, Glob)
- Output delimited with `---SPEC_START---` / `---SPEC_END---`

**`build_spec_resume_prompt()`** — Resume with user feedback:
- Includes current draft_spec + user feedback
- Agent revises spec based on feedback

**`build_spec_refine_prompt()`** — Continue refining from dedicated page:
- Includes final_spec (possibly manually edited by user)
- Agent proposes improvements or asks clarifying questions

### Spec Output Parsing

Parser detects `---SPEC_START---` / `---SPEC_END---` delimiters in stdout:
1. Extracts content between delimiters
2. Saves as `draft_spec` in DB
3. Emits `spec_ready` SSE event

Fallback: if no delimiters found, uses full output as spec.

### Agent Integration

Reuses `agent_service.start_agent()` as-is — `spec_id` is passed where `task_id` would normally go (both are UUIDs). SSE events emit with `spec_id` as key.

On agent completion:
- Success → parse spec, save `draft_spec`, status stays `refining`
- Failure → status `failed`, emit error SSE

### Approval Flow

`POST /specs/:id/approve`:
1. Takes `final_spec` (user-edited) or `draft_spec` (if no manual edit)
2. Creates new Task:
   - `repository_id`: from spec
   - `user_input`: the approved spec text
   - `title`: spec.title
   - `description`: the approved spec text
   - `agent_type`, `agent_model`: from spec
   - `status`: "approved"
3. Updates spec: `task_id = new_task.id`, `status = "approved"`, `approved_at = now`
4. Returns the created task

## Frontend

### Routes

```
/specs              — List + drawer (under mainLayoutRoute)
/specs/:specId      — Dedicated spec page (under mainLayoutRoute)
```

### Sidebar

New nav item between "Board" and "Tasks":
```
Board  | Specs | Tasks
```

### Page `/specs` — List + Drawer

**Left panel (list):**
- Header "Specs" + "+ New Spec" button
- Status filter (draft, refining, approved, all)
- List items: title, status badge, repo, date, truncated preview

**Right drawer (on item click):**
- Tab "Chat": user/agent messages, feedback input, activity indicator, collapsible logs
- When agent finishes: "View Spec →" button navigates to `/specs/:specId`

### Page `/specs/:specId` — Dedicated

- Editable title in header, status badge, repo name
- Main body: markdown editor with edit/preview toggle
- Actions:
  - "Continue Refining" → navigates back to `/specs`, opens drawer, relaunches agent
  - "Send to Code" → approves spec, creates Task, shows link/navigates to board

### Feature Module Structure

```
features/specs/
├── components/
│   ├── create-spec-dialog.tsx
│   ├── spec-list.tsx
│   ├── spec-list-item.tsx
│   ├── spec-drawer.tsx
│   ├── spec-chat.tsx
│   ├── spec-detail.tsx
│   ├── spec-editor.tsx
│   └── spec-status-badge.tsx
├── hooks/
│   ├── query-keys.ts
│   ├── use-specs.ts
│   ├── use-spec.ts
│   ├── use-spec-mutations.ts
│   ├── use-spec-sse.ts
│   └── use-spec-actions.ts
├── stores/
│   └── spec-ui-store.ts
└── types/
    └── index.ts
```

## What Doesn't Change

- `agent_service.rs` — used as-is (spec_id as generic task_id)
- `SSEEmitter` / `task_logs` — reused (spec_id as entity key)
- Task lifecycle — untouched, task is created clean on approval
- Board, task detail, diff views — no changes
- Existing CLI agent command building — no changes
