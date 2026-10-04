use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use axum::response::IntoResponse;
use syn::visit::Visit;
use syn::{File, ItemUse, Path as SynPath, UseTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Subsystem {
    Providers,
    AgentCore,
    Agent,
    Capabilities,
    Application,
    Infrastructure,
    Transport,
    Bootstrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Facade,
    ContractModel,
    ConsumerPort,
    Service,
    Adapter,
    Transport,
    CompositionRoot,
}

const LEGACY_ALLOWLIST: &[(&str, &str)] = &[];
const LEGACY_ROOTS: &[&str] = &[
    "app.rs",
    "app",
    "routes.rs",
    "routes",
    "db.rs",
    "db",
    "models.rs",
    "contracts",
    "ports",
    "state.rs",
    "config.rs",
    "auth.rs",
    "auth",
    "llm.rs",
    "llm",
    "error.rs",
    "commands.rs",
    "files.rs",
];
const LEGACY_PATHS: &[&str] = &[
    "crate::app",
    "crate::routes",
    "crate::db",
    "crate::models",
    "crate::contracts",
    "crate::ports",
    "crate::state",
    "crate::config",
    "crate::llm",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    file: String,
    dependency: String,
    subsystem: Subsystem,
    role: Role,
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

fn dependency_matches(path: &str, dependency: &str) -> bool {
    path == dependency || path.starts_with(&format!("{dependency}::"))
}

fn classify(relative: &str) -> Option<(Subsystem, Role)> {
    let subsystem = match relative.split('/').next()? {
        "providers.rs" | "providers" => Subsystem::Providers,
        "agent_core.rs" | "agent_core" => Subsystem::AgentCore,
        "agent.rs" | "agent" => Subsystem::Agent,
        "capabilities.rs" | "capabilities" => Subsystem::Capabilities,
        "application.rs" | "application" => Subsystem::Application,
        "infrastructure.rs" | "infrastructure" => Subsystem::Infrastructure,
        "transport.rs" | "transport" => Subsystem::Transport,
        "bootstrap.rs" | "bootstrap" => Subsystem::Bootstrap,
        _ => return None,
    };
    let filename = relative.rsplit('/').next().unwrap_or(relative);
    let role = if !relative.contains('/') {
        Role::Facade
    } else if subsystem == Subsystem::Bootstrap {
        Role::CompositionRoot
    } else if subsystem == Subsystem::Transport {
        Role::Transport
    } else if subsystem == Subsystem::Infrastructure
        || relative.starts_with("providers/api/")
        || relative.starts_with("providers/builtin/")
        || relative == "providers/client.rs"
    {
        Role::Adapter
    } else if filename == "ports.rs" {
        Role::ConsumerPort
    } else if matches!(filename, "model.rs" | "error.rs" | "event.rs" | "types.rs") {
        Role::ContractModel
    } else {
        Role::Service
    };
    Some((subsystem, role))
}

fn denied_dependencies(subsystem: Subsystem, role: Role) -> &'static [&'static str] {
    match subsystem {
        Subsystem::Providers if role != Role::Adapter => &[
            "axum",
            "tauri",
            "sqlx",
            "reqwest",
            "crate::agent",
            "crate::agent_core",
            "crate::application",
            "crate::capabilities",
            "crate::infrastructure",
            "crate::transport",
            "crate::bootstrap",
        ],
        Subsystem::Providers => &[
            "axum",
            "tauri",
            "sqlx",
            "crate::agent",
            "crate::agent_core",
            "crate::application",
            "crate::capabilities",
            "crate::infrastructure",
            "crate::transport",
            "crate::bootstrap",
        ],
        Subsystem::AgentCore => &[
            "axum",
            "tauri",
            "sqlx",
            "reqwest",
            "crate::agent",
            "crate::application",
            "crate::capabilities",
            "crate::infrastructure",
            "crate::transport",
            "crate::bootstrap",
        ],
        Subsystem::Agent => &[
            "axum",
            "tauri",
            "sqlx",
            "crate::application",
            "crate::infrastructure",
            "crate::transport",
            "crate::bootstrap",
        ],
        Subsystem::Capabilities => &[
            "axum",
            "tauri",
            "sqlx",
            "reqwest",
            "crate::agent",
            "crate::application",
            "crate::infrastructure",
            "crate::transport",
            "crate::bootstrap",
        ],
        Subsystem::Application => &[
            "axum",
            "tauri",
            "sqlx",
            "reqwest",
            "jsonwebtoken",
            "bcrypt",
            "crate::infrastructure",
            "crate::transport",
            "crate::bootstrap",
        ],
        Subsystem::Infrastructure => &["axum", "tauri", "crate::transport", "crate::bootstrap"],
        Subsystem::Transport => &[
            "sqlx",
            "reqwest",
            "crate::bootstrap",
            "crate::infrastructure",
            "crate::agent::tools",
            "crate::providers::api",
            "crate::providers::builtin",
        ],
        Subsystem::Bootstrap => &[],
    }
}

fn infrastructure_service_dependency(path: &str) -> bool {
    path == "crate::application::chat_service::ChatApplicationService"
        || path.ends_with("ApplicationService")
        || path.contains("::service::")
}

fn analyze_source(
    subsystem: Subsystem,
    role: Role,
    file: &str,
    source: &str,
) -> Result<Vec<Violation>, String> {
    let syntax: File = syn::parse_file(source).map_err(|error| format!("{file}: {error}"))?;
    let mut collector = PathCollector::default();
    collector.visit_file(&syntax);
    let mut dependencies = BTreeSet::new();
    for path in collector.paths {
        let denied = denied_dependencies(subsystem, role)
            .iter()
            .any(|dependency| dependency_matches(&path, dependency));
        let denied = denied
            || (subsystem == Subsystem::Infrastructure
                && dependency_matches(&path, "crate::application")
                && infrastructure_service_dependency(&path));
        if denied {
            dependencies.insert(path);
        }
    }
    Ok(dependencies
        .into_iter()
        .map(|dependency| Violation {
            file: file.into(),
            dependency,
            subsystem,
            role,
        })
        .collect())
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
fn architecture_boundaries_enforce_subsystem_and_role_matrix_without_allowlist() {
    assert!(LEGACY_ALLOWLIST.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for path in rust_files(&root) {
        let relative = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let Some((subsystem, role)) = classify(&relative) else {
            continue;
        };
        let source = fs::read_to_string(&path).unwrap();
        violations.extend(analyze_source(subsystem, role, &relative, &source).unwrap());
    }
    assert!(
        violations.is_empty(),
        "architecture boundary violations:\n{}",
        violations
            .iter()
            .map(|item| format!(
                "{} ({:?}/{:?}) -> {}",
                item.file, item.subsystem, item.role, item.dependency
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn architecture_rule_engine_detects_alias_grouped_multiline_and_full_paths() {
    let alias = r#"use crate::infrastructure::{persistence as storage};"#;
    let violations =
        analyze_source(Subsystem::Transport, Role::Transport, "fixture.rs", alias).unwrap();
    assert!(violations.iter().any(|item| {
        dependency_matches(&item.dependency, "crate::infrastructure::persistence")
    }));

    let full_path = r#"fn load() { let _ = sqlx::query("SELECT 1"); }"#;
    assert!(
        !analyze_source(
            Subsystem::Application,
            Role::Service,
            "fixture.rs",
            full_path,
        )
        .unwrap()
        .is_empty()
    );

    let grouped =
        r#"use crate::{application::chat_service::ChatApplicationService, transport::Http};"#;
    assert!(
        analyze_source(Subsystem::AgentCore, Role::Service, "fixture.rs", grouped,)
            .unwrap()
            .len()
            >= 2
    );
}

#[test]
fn architecture_rule_engine_accepts_consumer_owned_ports_and_composition_root() {
    let adapter = r#"
        use crate::application::assets::{Asset, AssetRepository};
        use crate::application::identity::{Account, AccountRepository};
    "#;
    assert!(
        analyze_source(
            Subsystem::Infrastructure,
            Role::Adapter,
            "fixture.rs",
            adapter,
        )
        .unwrap()
        .is_empty()
    );

    let service = r#"use crate::application::assets::AssetApplicationService;"#;
    assert!(
        !analyze_source(
            Subsystem::Infrastructure,
            Role::Adapter,
            "fixture.rs",
            service,
        )
        .unwrap()
        .is_empty()
    );

    let bootstrap = r#"
        use crate::{application::chat_service::ChatApplicationService, infrastructure::identity::AuthEndpoints};
    "#;
    assert!(
        analyze_source(
            Subsystem::Bootstrap,
            Role::CompositionRoot,
            "fixture.rs",
            bootstrap,
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn final_source_tree_has_only_approved_roots_and_no_mod_rs_or_global_service_state() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mod_files = rust_files(&root)
        .into_iter()
        .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some("mod.rs"))
        .collect::<Vec<_>>();
    assert!(mod_files.is_empty(), "mod.rs remains: {mod_files:?}");

    for legacy in LEGACY_ROOTS {
        assert!(!root.join(legacy).exists(), "legacy root remains: {legacy}");
    }
    let allowed_roots = [
        "agent",
        "agent.rs",
        "agent_core",
        "agent_core.rs",
        "application",
        "application.rs",
        "bootstrap",
        "bootstrap.rs",
        "capabilities",
        "capabilities.rs",
        "infrastructure",
        "infrastructure.rs",
        "providers",
        "providers.rs",
        "transport",
        "transport.rs",
        "lib.rs",
        "main.rs",
    ];
    for entry in fs::read_dir(&root).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            allowed_roots.contains(&name.as_str()),
            "unexpected crate root: {name}"
        );
    }
    for forbidden in [
        "common",
        "common.rs",
        "shared",
        "shared.rs",
        "utils",
        "utils.rs",
    ] {
        assert!(
            !root.join(forbidden).exists(),
            "generic root added: {forbidden}"
        );
    }

    for path in rust_files(&root) {
        let source = fs::read_to_string(&path).unwrap();
        let syntax = syn::parse_file(&source).unwrap();
        let mut collector = PathCollector::default();
        collector.visit_file(&syntax);
        for legacy in LEGACY_PATHS {
            assert!(
                !collector
                    .paths
                    .iter()
                    .any(|path| dependency_matches(path, legacy)),
                "{} still references {legacy}",
                path.display()
            );
        }
        assert!(
            !source.contains("AnyPool"),
            "{} uses AnyPool",
            path.display()
        );
        assert!(
            !source.contains("OnceLock"),
            "{} uses global OnceLock state",
            path.display()
        );
    }
}

#[tokio::test]
async fn http_error_wire_contract_is_stable_and_internal_details_are_redacted() {
    use revue_office_lib::transport::http::error::AppError;

    async fn response(error: AppError) -> (axum::http::StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    assert_eq!(
        response(AppError::Unauthorized).await,
        (
            axum::http::StatusCode::UNAUTHORIZED,
            serde_json::json!({"detail":"未认证"})
        )
    );
    assert_eq!(
        response(AppError::Forbidden).await,
        (
            axum::http::StatusCode::FORBIDDEN,
            serde_json::json!({"detail":"无权访问"})
        )
    );
    assert_eq!(
        response(AppError::BadRequest("输入无效".into())).await,
        (
            axum::http::StatusCode::BAD_REQUEST,
            serde_json::json!({"detail":"参数错误: 输入无效"})
        )
    );
    assert_eq!(
        response(AppError::NotFound("会话不存在".into())).await,
        (
            axum::http::StatusCode::NOT_FOUND,
            serde_json::json!({"detail":"未找到: 会话不存在"})
        )
    );
    assert_eq!(
        response(AppError::Internal(anyhow::anyhow!(
            "SELECT secret FROM users"
        )))
        .await,
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"detail":"内部错误"})
        )
    );
}

#[test]
fn office_agent_registry_and_http_state_remain_instance_scoped() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let agent = fs::read_to_string(root.join("agent/tools.rs")).unwrap();
    let state = fs::read_to_string(root.join("transport/http/state.rs")).unwrap();
    let bootstrap = fs::read_to_string(root.join("bootstrap.rs")).unwrap();
    assert!(!agent.contains("static REGISTRY"));
    assert!(!agent.contains("Lazy<ToolRegistry"));
    for forbidden in [
        "SqlitePool",
        "MySqlPool",
        "Repository",
        "CredentialStore",
        "ToolRegistry",
    ] {
        assert!(!state.contains(forbidden), "HttpState exposes {forbidden}");
    }
    assert!(bootstrap.contains("infrastructure::build_identity"));
    assert!(bootstrap.contains("application::build"));
    assert!(bootstrap.contains("http::build"));
}

#[test]
fn credential_and_tool_helpers_have_no_plaintext_or_legacy_config_fallback() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let config = fs::read_to_string(root.join("bootstrap/config.rs")).unwrap();
    for field in [
        "pub llm_api_key:",
        "pub llm_api_keys:",
        "pub llm_text_api_key:",
        "pub llm_image_api_key:",
        "pub llm_video_api_key:",
        "pub baidu_mcp_api_key:",
    ] {
        assert!(!config.contains(field));
    }
    let helper = fs::read_to_string(root.join("agent/tools/chat_provider.rs")).unwrap();
    for forbidden in [
        "crate::config",
        "crate::state",
        "crate::db",
        "bearer_auth",
        "API_KEY_ROUND_ROBIN",
    ] {
        assert!(
            !helper.contains(forbidden),
            "Tool helper contains {forbidden}"
        );
    }
    assert!(helper.contains("provider_resolver"));
    let selector = fs::read_to_string(root.join("bootstrap/providers.rs")).unwrap();
    assert!(selector.contains("credentials.revision()"));
    assert!(!selector.contains("keys.hash"));
}

#[test]
fn presentation_capability_owns_renderer_models_and_ports() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let capability = fs::read_to_string(root.join("capabilities/presentation/model.rs")).unwrap();
    let renderer = fs::read_to_string(root.join("infrastructure/export/pptx.rs")).unwrap();
    let export = fs::read_to_string(root.join("infrastructure/export.rs")).unwrap();
    assert!(capability.contains("pub struct PresentationProject"));
    assert!(renderer.contains("PresentationProject"));
    assert!(renderer.contains("PresentationSlide"));
    assert!(renderer.contains("PresentationElement"));
    assert!(!renderer.contains("crate::models"));
    assert!(!export.contains("legacy_project"));
    assert!(!export.contains("serde_json::to_value(project)"));
}
