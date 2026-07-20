//! Stack detector service for analyzing a repository to detect its technology stack.
//!
//! Port of `packages/server/src/services/stack-detector.service.ts`.
//!
//! Detects:
//! - Framework (React, Next.js, Vue, Angular, Svelte, Express, NestJS, etc.)
//! - State management (Zustand, Redux, Jotai, etc.)
//! - Styling (Tailwind, styled-components, Chakra UI, etc.)
//! - Testing (Vitest, Jest, Playwright, Cypress, etc.)
//!
//! Works on local filesystem paths (no GitHub API required).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::info;

use crate::error::AppError;
use crate::models::repository::DetectedStack;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Response from stack detection, including confidence scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackDetectionResponse {
    pub detected_stack: DetectedStack,
    pub confidence: StackConfidence,
}

/// Confidence scores for each detected category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackConfidence {
    pub framework: f64,
    pub state_management: f64,
    pub styling: f64,
    pub testing: f64,
}

/// Simplified package.json representation for dependency scanning.
#[derive(Debug, Deserialize)]
struct PackageJson {
    #[serde(default)]
    dependencies: HashMap<String, String>,
    #[serde(rename = "devDependencies", default)]
    dev_dependencies: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Detection patterns (mirrors the TS constants)
// ---------------------------------------------------------------------------

fn framework_patterns() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("Next.js", vec!["next", "@next/"]),
        ("React", vec!["react", "react-dom"]),
        ("Vue.js", vec!["vue", "@vue/"]),
        ("Angular", vec!["@angular/core", "@angular/"]),
        ("Svelte", vec!["svelte", "@sveltejs/"]),
        ("Nuxt.js", vec!["nuxt", "@nuxt/"]),
        ("Remix", vec!["@remix-run/"]),
        ("Astro", vec!["astro"]),
        ("Express", vec!["express"]),
        ("NestJS", vec!["@nestjs/core", "@nestjs/"]),
        ("Fastify", vec!["fastify"]),
        ("Hono", vec!["hono"]),
    ]
}

fn state_management_patterns() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("Zustand", vec!["zustand"]),
        ("Redux", vec!["redux", "@reduxjs/toolkit", "react-redux"]),
        ("Jotai", vec!["jotai"]),
        ("Recoil", vec!["recoil"]),
        ("MobX", vec!["mobx", "mobx-react"]),
        ("Pinia", vec!["pinia"]),
        ("Vuex", vec!["vuex"]),
        (
            "TanStack Query",
            vec!["@tanstack/react-query", "react-query"],
        ),
        ("SWR", vec!["swr"]),
        ("XState", vec!["xstate"]),
    ]
}

fn styling_patterns() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("Tailwind CSS", vec!["tailwindcss", "@tailwindcss/"]),
        ("styled-components", vec!["styled-components"]),
        ("Emotion", vec!["@emotion/react", "@emotion/styled"]),
        ("Sass", vec!["sass", "node-sass"]),
        ("Less", vec!["less"]),
        ("Chakra UI", vec!["@chakra-ui/react"]),
        ("Material UI", vec!["@mui/material", "@material-ui/"]),
        ("Ant Design", vec!["antd"]),
    ]
}

fn testing_patterns() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("Vitest", vec!["vitest"]),
        ("Jest", vec!["jest", "@jest/"]),
        ("Playwright", vec!["@playwright/test", "playwright"]),
        ("Cypress", vec!["cypress"]),
        ("Testing Library", vec!["@testing-library/"]),
        ("Mocha", vec!["mocha"]),
        ("AVA", vec!["ava"]),
    ]
}

// ---------------------------------------------------------------------------
// Detection logic
// ---------------------------------------------------------------------------

/// Detects a single category (framework, styling, etc.) from the package.json deps.
fn detect_category(dep_names: &[String], patterns: &[(&str, Vec<&str>)]) -> (Option<String>, f64) {
    for (name, search_patterns) in patterns {
        if search_patterns.is_empty() {
            continue;
        }
        for pattern in search_patterns {
            let found = dep_names
                .iter()
                .any(|dep| dep == pattern || dep.starts_with(pattern));
            if found {
                let confidence = if dep_names.iter().any(|d| d == pattern) {
                    1.0
                } else {
                    0.9
                };
                return (Some(name.to_string()), confidence);
            }
        }
    }
    (None, 0.0)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detects the technology stack of a local repository by reading `package.json`,
/// `Cargo.toml`, `go.mod`, etc. from the filesystem.
pub async fn detect_stack(repo_path: &Path) -> Result<StackDetectionResponse, AppError> {
    info!(repo_path = %repo_path.display(), "Detecting stack for local repository");

    let package_json = read_package_json(repo_path).await;
    let root_files = list_root_files(repo_path).await;

    // Detect from package.json dependencies
    let dep_names = match &package_json {
        Some(pj) => {
            let mut names: Vec<String> = pj.dependencies.keys().cloned().collect();
            names.extend(pj.dev_dependencies.keys().cloned());
            names
        }
        None => Vec::new(),
    };

    let (mut framework, framework_conf) = detect_category(&dep_names, &framework_patterns());
    let (state_mgmt, state_conf) = detect_category(&dep_names, &state_management_patterns());
    let (mut styling, mut styling_conf) = detect_category(&dep_names, &styling_patterns());
    let (testing, testing_conf) = detect_category(&dep_names, &testing_patterns());

    // File-based detection for styling
    if styling.is_none()
        && root_files
            .iter()
            .any(|f| f == "tailwind.config.js" || f == "tailwind.config.ts")
    {
        styling = Some("Tailwind CSS".to_string());
        styling_conf = 0.9;
    }

    // shadcn/ui detection via components.json
    if root_files.iter().any(|f| f == "components.json") {
        if let Some(ref existing) = styling {
            styling = Some(format!("{existing}, shadcn/ui"));
        } else {
            styling = Some("shadcn/ui".to_string());
            styling_conf = 0.9;
        }
    }

    // If no JS framework detected, try other project types
    if framework.is_none() {
        if root_files.iter().any(|f| f == "Cargo.toml") {
            framework = Some("Rust".to_string());
        } else if root_files.iter().any(|f| f == "go.mod") {
            framework = Some("Go".to_string());
        } else if root_files
            .iter()
            .any(|f| f == "requirements.txt" || f == "pyproject.toml")
        {
            framework = Some("Python".to_string());
        } else if root_files.iter().any(|f| f == "Gemfile") {
            framework = Some("Ruby".to_string());
        } else if root_files
            .iter()
            .any(|f| f == "pom.xml" || f == "build.gradle")
        {
            framework = Some("Java".to_string());
        }
    }

    let detected_stack = DetectedStack {
        framework,
        state_management: state_mgmt,
        styling,
        testing,
    };

    info!(
        repo_path = %repo_path.display(),
        ?detected_stack,
        "Stack detected successfully"
    );

    Ok(StackDetectionResponse {
        detected_stack,
        confidence: StackConfidence {
            framework: framework_conf,
            state_management: state_conf,
            styling: styling_conf,
            testing: testing_conf,
        },
    })
}

/// Reads `package.json` from a directory, returning `None` on any error.
async fn read_package_json(dir: &Path) -> Option<PackageJson> {
    let path = dir.join("package.json");
    let content = fs::read_to_string(&path).await.ok()?;
    serde_json::from_str(&content).ok()
}

/// Lists the names of files and directories in the root of the given path.
async fn list_root_files(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return names,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
    }
    names
}

/// Detects the primary language of a repository based on root files.
/// Used by the local scan service.
pub async fn detect_language(repo_path: &Path, has_package_json: bool) -> Option<String> {
    if has_package_json {
        let tsconfig = repo_path.join("tsconfig.json");
        if fs::metadata(&tsconfig).await.is_ok() {
            return Some("TypeScript".to_string());
        }
        return Some("JavaScript".to_string());
    }

    let language_files: &[(&str, &str)] = &[
        ("Cargo.toml", "Rust"),
        ("go.mod", "Go"),
        ("requirements.txt", "Python"),
        ("pyproject.toml", "Python"),
        ("Gemfile", "Ruby"),
        ("pom.xml", "Java"),
        ("build.gradle", "Java"),
        ("composer.json", "PHP"),
    ];

    for (file, lang) in language_files {
        if fs::metadata(repo_path.join(file)).await.is_ok() {
            return Some(lang.to_string());
        }
    }

    None
}
