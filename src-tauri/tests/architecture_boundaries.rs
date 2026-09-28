use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;
use syn::{File, ItemUse, Path as SynPath, UseTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Contract,
    Port,
    Capability,
    Agent,
    Application,
    Transport,
    Infrastructure,
    App,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AllowEntry {
    file: &'static str,
    dependency: &'static str,
    reason: &'static str,
}

// Only HTTP modules not yet migrated to Application Services may bypass the
// Transport -> legacy persistence rule. Migrated chat/session files are absent.
const LEGACY_ALLOWLIST: &[AllowEntry] = &[
    AllowEntry {
        file: "routes/auth.rs",
        dependency: "crate::db",
        reason: "authentication migration is outside this slice",
    },
    AllowEntry {
        file: "routes/auth.rs",
        dependency: "crate::state",
        reason: "authentication migration is outside this slice",
    },
    AllowEntry {
        file: "routes/dashboard.rs",
        dependency: "crate::db",
        reason: "dashboard migration is outside this slice",
    },
    AllowEntry {
        file: "routes/dashboard.rs",
        dependency: "crate::state",
        reason: "dashboard migration is outside this slice",
    },
    AllowEntry {
        file: "routes/file.rs",
        dependency: "crate::db",
        reason: "file API migration is outside this slice",
    },
    AllowEntry {
        file: "routes/file.rs",
        dependency: "crate::state",
        reason: "file API migration is outside this slice",
    },
    AllowEntry {
        file: "routes/notification.rs",
        dependency: "crate::db",
        reason: "notification migration is outside this slice",
    },
    AllowEntry {
        file: "routes/notification.rs",
        dependency: "crate::state",
        reason: "notification migration is outside this slice",
    },
    AllowEntry {
        file: "routes/project.rs",
        dependency: "crate::db",
        reason: "project API migration is outside this slice",
    },
    AllowEntry {
        file: "routes/project.rs",
        dependency: "crate::state",
        reason: "project API migration is outside this slice",
    },
    AllowEntry {
        file: "routes/settings.rs",
        dependency: "crate::db",
        reason: "settings migration is outside this slice",
    },
    AllowEntry {
        file: "routes/settings.rs",
        dependency: "crate::state",
        reason: "settings migration is outside this slice",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    file: String,
    dependency: String,
    layer: Layer,
}

#[derive(Default)]
struct PathCollector {
    paths: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for PathCollector {
    fn visit_path(&mut self, path: &'ast SynPath) {
        self.paths.insert(path_to_string(path));
        syn::visit::visit_path(self, path);
    }

    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        collect_use_tree(&item.tree, Vec::new(), &mut self.paths);
        syn::visit::visit_item_use(self, item);
    }
}

fn path_to_string(path: &SynPath) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn collect_use_tree(tree: &UseTree, prefix: Vec<String>, paths: &mut BTreeSet<String>) {
    match tree {
        UseTree::Path(path) => {
            let mut next = prefix;
            next.push(path.ident.to_string());
            collect_use_tree(&path.tree, next, paths);
        }
        UseTree::Name(name) => {
            let mut path = prefix;
            path.push(name.ident.to_string());
            paths.insert(path.join("::"));
        }
        UseTree::Rename(rename) => {
            let mut path = prefix;
            path.push(rename.ident.to_string());
            paths.insert(path.join("::"));
        }
        UseTree::Glob(_) => {
            let mut path = prefix;
            path.push("*".into());
            paths.insert(path.join("::"));
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_tree(item, prefix.clone(), paths);
            }
        }
    }
}

fn forbidden(layer: Layer) -> &'static [&'static str] {
    match layer {
        Layer::Contract => &[
            "axum",
            "tauri",
            "sqlx",
            "crate::app",
            "crate::application",
            "crate::agent",
            "crate::capabilities",
            "crate::infrastructure",
            "crate::ports",
            "crate::routes",
            "crate::transport",
            "crate::db",
            "crate::state",
        ],
        Layer::Port => &[
            "axum",
            "tauri",
            "sqlx",
            "crate::app",
            "crate::application",
            "crate::agent",
            "crate::capabilities",
            "crate::infrastructure",
            "crate::routes",
            "crate::transport",
            "crate::db",
            "crate::state",
        ],
        Layer::Capability => &[
            "axum",
            "tauri",
            "sqlx",
            "crate::app",
            "crate::application",
            "crate::agent",
            "crate::infrastructure",
            "crate::routes",
            "crate::transport",
            "crate::db",
            "crate::state",
        ],
        Layer::Agent => &[
            "axum",
            "tauri",
            "sqlx",
            "crate::app",
            "crate::application",
            "crate::infrastructure",
            "crate::routes",
            "crate::transport",
            "crate::db",
            "crate::state",
        ],
        Layer::Application => &[
            "axum",
            "tauri",
            "sqlx",
            "crate::app",
            "crate::infrastructure",
            "crate::routes",
            "crate::transport",
            "crate::db",
            "crate::state",
        ],
        Layer::Transport => &[
            "sqlx",
            "crate::infrastructure::persistence",
            "crate::db",
            "crate::state",
            "crate::agent::event",
            "crate::agent::runtime",
        ],
        Layer::Infrastructure => &["crate::application", "crate::routes", "crate::transport"],
        Layer::App => &[],
    }
}

fn dependency_matches(path: &str, forbidden: &str) -> bool {
    path == forbidden || path.starts_with(&format!("{forbidden}::"))
}

fn analyze_source(layer: Layer, file: &str, source: &str) -> Result<Vec<Violation>, String> {
    let syntax: File = syn::parse_file(source).map_err(|error| format!("{file}: {error}"))?;
    let mut collector = PathCollector::default();
    collector.visit_file(&syntax);
    let mut violations = BTreeSet::new();
    for path in collector.paths {
        for denied in forbidden(layer) {
            if dependency_matches(&path, denied) {
                violations.insert((path.clone(), *denied));
            }
        }
    }
    Ok(violations
        .into_iter()
        .map(|(dependency, _)| Violation {
            file: file.into(),
            dependency,
            layer,
        })
        .collect())
}

fn layer_for(relative: &str) -> Option<Layer> {
    let first = relative.split('/').next()?;
    match first {
        "contracts" => Some(Layer::Contract),
        "ports" => Some(Layer::Port),
        "capabilities" => Some(Layer::Capability),
        "agent" => Some(Layer::Agent),
        "application" => Some(Layer::Application),
        "transport" | "routes" => Some(Layer::Transport),
        "infrastructure" => Some(Layer::Infrastructure),
        "app" => Some(Layer::App),
        _ => None,
    }
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn architecture_boundaries_enforce_dependency_matrix_and_exact_legacy_allowlist() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    let mut used_allowlist = HashSet::new();

    for path in rust_files(&root) {
        let relative = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let Some(layer) = layer_for(&relative) else {
            continue;
        };
        let source = fs::read_to_string(&path).unwrap();
        for violation in analyze_source(layer, &relative, &source).unwrap() {
            let allowed = LEGACY_ALLOWLIST.iter().find(|entry| {
                entry.file == relative
                    && dependency_matches(&violation.dependency, entry.dependency)
            });
            if let Some(entry) = allowed {
                assert!(
                    !entry.reason.trim().is_empty(),
                    "allowlist reason is required"
                );
                used_allowlist.insert((entry.file, entry.dependency));
            } else {
                violations.push(violation);
            }
        }
    }

    let stale = LEGACY_ALLOWLIST
        .iter()
        .filter(|entry| !used_allowlist.contains(&(entry.file, entry.dependency)))
        .map(|entry| format!("{} -> {}", entry.file, entry.dependency))
        .collect::<Vec<_>>();
    assert!(
        stale.is_empty(),
        "stale architecture allowlist entries:\n{}",
        stale.join("\n")
    );
    assert!(
        !LEGACY_ALLOWLIST.iter().any(|entry| {
            matches!(entry.file, "routes/chat.rs" | "routes/session.rs")
                || entry.file.starts_with("transport/")
                || entry.file.starts_with("application/")
                || entry.file.starts_with("capabilities/")
        }),
        "migrated files must not appear in the architecture allowlist"
    );
    assert!(
        violations.is_empty(),
        "architecture boundary violations:\n{}",
        violations
            .iter()
            .map(|violation| format!(
                "{} ({:?}) -> {}",
                violation.file, violation.layer, violation.dependency
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn architecture_boundaries_rule_engine_detects_alias_multiline_and_full_paths() {
    let alias_and_multiline = r#"
        use crate::infrastructure::{
            persistence as storage,
        };
        fn load() { let _ = storage::load(); }
    "#;
    let transport = analyze_source(Layer::Transport, "fixture.rs", alias_and_multiline).unwrap();
    assert!(transport.iter().any(|violation| {
        dependency_matches(&violation.dependency, "crate::infrastructure::persistence")
    }));

    let full_path = r#"fn load() { let _ = sqlx::query("SELECT 1"); }"#;
    let capability = analyze_source(Layer::Capability, "fixture.rs", full_path).unwrap();
    assert!(capability
        .iter()
        .any(|violation| dependency_matches(&violation.dependency, "sqlx")));

    let agent_framework = r#"use axum::{extract::State, response::Response};"#;
    assert!(!analyze_source(Layer::Agent, "fixture.rs", agent_framework)
        .unwrap()
        .is_empty());

    let agent_adapter =
        r#"use crate::infrastructure::llm::presentation::ConfiguredPresentationLlm;"#;
    assert!(!analyze_source(Layer::Agent, "fixture.rs", agent_adapter)
        .unwrap()
        .is_empty());

    let infrastructure_application =
        r#"use crate::application::artifact_service::ArtifactService;"#;
    assert!(!analyze_source(
        Layer::Infrastructure,
        "fixture.rs",
        infrastructure_application,
    )
    .unwrap()
    .is_empty());
}

#[test]
fn architecture_boundaries_rule_engine_accepts_allowed_direction() {
    let source = r#"
        use crate::contracts::presentation::{PresentationPlan, PresentationProgress};
        use crate::ports::llm::PresentationLlm;
        fn plan(_: &dyn PresentationLlm) -> Option<PresentationPlan> { None }
    "#;
    assert!(analyze_source(Layer::Capability, "fixture.rs", source)
        .unwrap()
        .is_empty());
}
