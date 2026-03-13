/// Forbidden-actions footer appended to all execution prompts.
const FORBIDDEN_SECTION: &str = r#"
## Allowed File Operations
- You **CAN** create, edit, move, rename, and **delete** files inside your working directory
- Deleting files is a normal part of refactoring -- do it when the task requires it
- Use Bash (`rm`, `mv`, `mkdir`, etc.) freely for file operations **within** your working directory

## FORBIDDEN -- Do NOT do any of the following
- **DO NOT** read, edit, delete, or navigate to files **outside** your working directory
- **DO NOT** run tests, builds, linters, or any verification commands
- **DO NOT** start dev servers or any process that listens on a port
- **DO NOT** run `npm run build`, `npm run test`, `npm run dev`, or similar commands
- **DO NOT** run `npx`, `node`, or any script that executes project code
- **DO NOT** use git commands (git add, git commit, git checkout, git switch, git branch, git push, etc.) -- the system manages version control automatically
- Your job is ONLY to write code -- verification and version control will be done separately"#;

/// Returns agent-specific execution instructions.
///
/// Some models (e.g., Codex) tend to create implementation plans before coding.
/// These instructions override that behavior to ensure direct execution.
fn build_agent_specific_instructions(agent_type: Option<&str>) -> &'static str {
    match agent_type {
        Some("codex") | Some("openrouter") => {
            r#"

## IMPORTANT -- Direct Execution Required
- **DO NOT** create an implementation plan, outline, or strategy before coding
- **DO NOT** list the steps you will take or describe your approach
- **Start coding immediately** -- read the relevant files and make changes right away
- Your output should be tool calls and code changes, not planning text"#
        }
        _ => "",
    }
}

/// Builds the workspace boundary section that restricts the agent to a specific directory.
fn build_workspace_section(workspace_path: Option<&str>) -> String {
    let Some(path) = workspace_path else {
        return String::new();
    };

    format!(
        r#"
## CRITICAL -- Workspace Boundary (MUST OBEY)
Your working directory is: `{path}`

**ABSOLUTE RULE -- NO EXCEPTIONS:**
- EVERY file path you use (Read, Edit, Write, Bash, Glob, Grep) MUST start with `{path}`
- NEVER use paths outside this directory -- not even parent directories
- NEVER traverse upward (e.g., `../`, `cd ..`, or removing path segments)
- NEVER run commands like `find`, `ls`, or `cat` on paths outside `{path}`
- The worktree IS the repository -- all project files are here. There is nothing useful above this directory.
- If you read a `.git` file pointing elsewhere, DO NOT follow that path -- it is internal infrastructure

**If you access ANY path that does not start with `{path}`, the task WILL be terminated.**"#
    )
}

/// Builds the repository context section from repo metadata.
///
/// In the TypeScript version this reads from a `Repository` object. Here we accept
/// an opaque string that the caller pre-formats (since the DB model may evolve).
fn build_repository_section(repo_context: &str) -> String {
    if repo_context.is_empty() {
        return String::new();
    }
    format!("\n{repo_context}\n")
}

/// Builds the main task prompt for a normal (non-resume, non-plan) execution.
///
/// Consolidates all context into a single comprehensive prompt for CLI agent execution.
pub fn build_task_prompt(
    title: &str,
    spec: &str,
    context_files: &[String],
    repo_context: Option<&str>,
    agent_type: Option<&str>,
    workspace_path: Option<&str>,
) -> String {
    let context_section = if context_files.is_empty() {
        String::new()
    } else {
        let files = context_files
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n## Context Files\nReview these files before making any changes -- they are directly relevant to your task:\n{files}\n"
        )
    };

    let repo_section = repo_context.map(build_repository_section).unwrap_or_default();
    let workspace_section = build_workspace_section(workspace_path);
    let agent_instructions = build_agent_specific_instructions(agent_type);

    format!(
        r#"You are an autonomous coding agent. Implement the following task in the current repository.
{workspace_section}
## Task
**Title:** {title}

**Specification:**
{spec}
{repo_section}{context_section}
## Workflow

1. **Explore** the codebase structure to understand the project layout and conventions
2. **Read** relevant files to understand existing patterns before making changes
3. **Implement** the changes step by step, following existing code style and conventions

## Guidelines
- Understand before changing: always read existing code before modifying it
- Follow existing patterns: match the coding style, naming conventions, and architecture
- Make minimal changes: only modify what is necessary for the task
{FORBIDDEN_SECTION}{agent_instructions}"#
    )
    .trim()
    .to_string()
}

/// Builds a resume prompt for when the agent needs to address reviewer feedback
/// on a previously submitted task.
pub fn build_resume_prompt(
    title: &str,
    spec: &str,
    feedback: &str,
    repo_context: Option<&str>,
    agent_type: Option<&str>,
    workspace_path: Option<&str>,
) -> String {
    let repo_section = repo_context.map(build_repository_section).unwrap_or_default();
    let workspace_section = build_workspace_section(workspace_path);
    let agent_instructions = build_agent_specific_instructions(agent_type);

    format!(
        r#"You are an autonomous coding agent. You previously worked on a task that received reviewer feedback. Address the feedback and complete the task.
{workspace_section}
## Task Context
**Title:** {title}

**Original Specification:**
{spec}
{repo_section}
## Reviewer Feedback
The reviewer has requested the following changes:

{feedback}

## Workflow

1. **Review** the current state of your previous changes in the working tree
2. **Understand** exactly what the reviewer is asking for
3. **Implement** the necessary modifications to address the feedback

## Guidelines
- Focus specifically on addressing the reviewer's feedback
- Make minimal additional changes beyond what is requested
{FORBIDDEN_SECTION}{agent_instructions}"#
    )
    .trim()
    .to_string()
}

/// Builds a prompt for initializing a completely empty repository from scratch.
pub fn build_empty_repo_prompt(
    title: &str,
    spec: &str,
    repo_context: Option<&str>,
    agent_type: Option<&str>,
    workspace_path: Option<&str>,
) -> String {
    let repo_section = repo_context.map(build_repository_section).unwrap_or_default();
    let workspace_section = build_workspace_section(workspace_path);
    let agent_instructions = build_agent_specific_instructions(agent_type);

    format!(
        r#"You are an autonomous coding agent. This repository is completely empty -- there are no files or commits yet. Your task is to create the initial project from scratch.
{workspace_section}
## Task
**Title:** {title}

**Specification:**
{spec}
{repo_section}
## Workflow

1. **Analyze** the requirements from the specification to determine the appropriate project type and tech stack
2. **Create the project structure** including:
   - Configuration files (package.json, tsconfig.json, etc. as appropriate)
   - A .gitignore file with sensible defaults
   - Source code directories and initial files
3. **Implement** the requested functionality as described in the specification

## Guidelines
- Start with the most fundamental files first (package.json or equivalent)
- Create directories before creating files within them
- All files should have proper content, not just placeholders
- Follow best practices for the chosen technology stack
{FORBIDDEN_SECTION}{agent_instructions}"#
    )
    .trim()
    .to_string()
}

/// Builds a plan-only prompt that instructs the agent to explore the codebase and
/// create a detailed implementation plan WITHOUT making any file changes.
pub fn build_plan_only_prompt(
    title: &str,
    spec: &str,
    context_files: &[String],
    repo_context: Option<&str>,
    workspace_path: Option<&str>,
) -> String {
    let context_section = if context_files.is_empty() {
        String::new()
    } else {
        let files = context_files
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n## Context Files\nReview these files -- they are directly relevant to your task:\n{files}\n"
        )
    };

    let repo_section = repo_context.map(build_repository_section).unwrap_or_default();
    let workspace_section = build_workspace_section(workspace_path);

    format!(
        r#"You are an autonomous coding agent in PLAN-ONLY mode. Your job is to explore the codebase and create a detailed implementation plan for the following task. You must NOT make any changes to files -- only read and analyze.
{workspace_section}
## Task
**Title:** {title}

**Specification:**
{spec}
{repo_section}{context_section}
## Your Mission

Create a comprehensive, step-by-step implementation plan. You should:

1. **Explore** the codebase structure to understand the project layout, architecture, and conventions
2. **Read** relevant files to understand existing patterns, dependencies, and integration points
3. **Analyze** what needs to change and identify potential risks or edge cases
4. **Output a detailed plan** that includes:
   - Which files need to be created, modified, or deleted
   - The exact changes needed in each file (describe the code changes clearly)
   - The order of changes and dependencies between them
   - Key design decisions and their rationale
   - Any potential risks or things to watch out for

## CRITICAL RULES
- **DO NOT** read, edit, or navigate to files outside your working directory
- **DO NOT** create, edit, write, or modify any files
- **DO NOT** run any commands that modify the filesystem
- **ONLY** use read-only operations: reading files, searching code, listing directories
- Your output IS the plan -- write it clearly so a developer can follow it step by step
- Be specific: include file paths, function names, and describe the actual code changes needed"#
    )
    .trim()
    .to_string()
}

/// Builds an implementation prompt that tells the agent to execute
/// a previously approved plan step by step.
pub fn build_implementation_prompt(
    title: &str,
    spec: &str,
    plan: &str,
    repo_context: Option<&str>,
    agent_type: Option<&str>,
    workspace_path: Option<&str>,
) -> String {
    let repo_section = repo_context.map(build_repository_section).unwrap_or_default();
    let workspace_section = build_workspace_section(workspace_path);
    let agent_instructions = build_agent_specific_instructions(agent_type);

    format!(
        r#"You are an autonomous coding agent. You have a previously approved implementation plan. Your job is to implement it step by step.
{workspace_section}
## Task
**Title:** {title}

**Specification:**
{spec}
{repo_section}
## Approved Implementation Plan

The following plan has been reviewed and approved by the user. Follow it closely:

{plan}

## Workflow

1. **Follow the plan** -- implement each step in the order specified

## Guidelines
- Follow the approved plan closely -- it has been reviewed and approved
- If you encounter unexpected issues not covered by the plan, use your best judgment
- Follow existing code style and conventions
- Make minimal changes beyond what the plan specifies
{FORBIDDEN_SECTION}{agent_instructions}"#
    )
    .trim()
    .to_string()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_prompt_contains_title_and_spec() {
        let prompt = build_task_prompt("Add login", "Implement login form", &[], None, None, None);
        assert!(prompt.contains("**Title:** Add login"));
        assert!(prompt.contains("Implement login form"));
        assert!(prompt.contains("FORBIDDEN"));
    }

    #[test]
    fn task_prompt_includes_context_files() {
        let files = vec!["src/auth.rs".to_string(), "src/main.rs".to_string()];
        let prompt = build_task_prompt("Fix auth", "Fix bug", &files, None, None, None);
        assert!(prompt.contains("- src/auth.rs"));
        assert!(prompt.contains("- src/main.rs"));
    }

    #[test]
    fn resume_prompt_includes_feedback() {
        let prompt = build_resume_prompt("Fix bug", "Spec", "Please add error handling", None, None, None);
        assert!(prompt.contains("Please add error handling"));
        assert!(prompt.contains("Reviewer Feedback"));
    }

    #[test]
    fn codex_agent_gets_direct_execution_instructions() {
        let prompt = build_task_prompt("Task", "Spec", &[], None, Some("codex"), None);
        assert!(prompt.contains("Direct Execution Required"));
    }

    #[test]
    fn workspace_section_is_included() {
        let prompt = build_task_prompt("Task", "Spec", &[], None, None, Some("/work/repo"));
        assert!(prompt.contains("Workspace Boundary"));
        assert!(prompt.contains("/work/repo"));
    }

    #[test]
    fn plan_only_prompt_forbids_file_changes() {
        let prompt = build_plan_only_prompt("Task", "Spec", &[], None, None);
        assert!(prompt.contains("PLAN-ONLY mode"));
        assert!(prompt.contains("DO NOT** create, edit, write, or modify any files"));
    }

    #[test]
    fn implementation_prompt_includes_plan() {
        let prompt = build_implementation_prompt("Task", "Spec", "1. Create file\n2. Edit file", None, None, None);
        assert!(prompt.contains("Approved Implementation Plan"));
        assert!(prompt.contains("1. Create file"));
    }
}
