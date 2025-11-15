extern crate proc_macro;
use proc_macro::TokenStream;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "../codegen_shared.rs"]
mod codegen_shared;

#[proc_macro]
pub fn generate_rpc_client(_input: TokenStream) -> TokenStream {
    let generated_code = match load_generated_code() {
        Ok(code) => code,
        Err(e) => {
            eprintln!(
                "Warning: Failed to load generated code: {}. Using runtime fallback.",
                e
            );
            generate_runtime_fallback_client()
        }
    };

    let tokens: proc_macro2::TokenStream = generated_code
        .parse()
        .expect("Failed to parse generated code");

    TokenStream::from(tokens)
}

#[proc_macro]
pub fn create_rpc_client(_input: TokenStream) -> TokenStream {
    generate_rpc_client(_input)
}

fn load_generated_code() -> Result<String, Box<dyn std::error::Error>> {
    if env::var("LAZ_DISABLE_AUTO_FETCH").is_err() {
        match fetch_latest_code_from_server() {
            Ok(code) => return Ok(code),
            Err(err) => {
                eprintln!(
                    "laz_client_macros: Failed to fetch fresh metadata, falling back to cache: {}",
                    err
                );
            }
        }
    }

    load_cached_code_from_disk()
}

fn fetch_latest_code_from_server() -> Result<String, Box<dyn std::error::Error>> {
    let server_url =
        env::var("LAZ_SERVER_URL").unwrap_or_else(|_| "http://localhost:5150".to_string());

    match codegen_shared::generate_client_code_from_server(&server_url) {
        Ok((code, _metadata)) => {
            cache_generated_code(&code);
            Ok(code)
        }
        Err(err) => Err(err),
    }
}

fn cache_generated_code(code: &str) {
    if let Ok(out_dir) = env::var("OUT_DIR") {
        let dest_path = PathBuf::from(out_dir).join("generated_rpc_client.rs");
        if let Err(err) = fs::write(&dest_path, code) {
            eprintln!(
                "laz_client_macros: Failed to cache generated client at {}: {}",
                dest_path.display(),
                err
            );
        }
    }
}

fn load_cached_code_from_disk() -> Result<String, Box<dyn std::error::Error>> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(env_path) = env::var("LAZ_CLIENT_GENERATED_CODE_PATH") {
        candidates.push(PathBuf::from(env_path));
    }

    if let Ok(out_dir) = env::var("OUT_DIR") {
        candidates.push(PathBuf::from(out_dir).join("generated_rpc_client.rs"));
    }

    for target_root in collect_target_roots() {
        for profile in ["debug", "release"] {
            if let Some(path) = find_generated_file_in_target(&target_root, profile) {
                candidates.push(path);
            }
        }
    }

    // Add additional search paths for common locations
    if let Ok(current_dir) = env::current_dir() {
        // Look in current directory's target
        candidates.push(
            current_dir
                .join("target")
                .join("debug")
                .join("build")
                .join("laz_client_macros-out")
                .join("generated_rpc_client.rs"),
        );
        candidates.push(
            current_dir
                .join("target")
                .join("release")
                .join("build")
                .join("laz_client_macros-out")
                .join("generated_rpc_client.rs"),
        );

        // Look for any laz_client_macros build directory
        for profile in ["debug", "release"] {
            let build_dir = current_dir.join("target").join(profile).join("build");
            if build_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&build_dir) {
                    for entry in entries.flatten() {
                        let entry_path = entry.path();
                        if entry_path
                            .file_name()
                            .and_then(|f| f.to_str())
                            .map(|name| name.starts_with("laz_client_macros-"))
                            .unwrap_or(false)
                        {
                            let generated_file =
                                entry_path.join("out").join("generated_rpc_client.rs");
                            candidates.push(generated_file);
                        }
                    }
                }
            }
        }
    }

    // Debug: Print all candidates being searched
    eprintln!("laz_client_macros: Searching for generated code in:");

    // Collect all existing files and their metadata
    let mut existing_files = Vec::new();
    for path in &candidates {
        eprintln!("  - {}", path.display());
        if path.exists() {
            if let Ok(metadata) = std::fs::metadata(&path) {
                if let Ok(created) = metadata.created() {
                    existing_files.push((path.clone(), created, metadata.len()));
                } else {
                    existing_files.push((
                        path.clone(),
                        std::time::SystemTime::now(),
                        metadata.len(),
                    ));
                }
            }
        }
    }

    // Sort by creation time (newest first) and size (larger files first, as they contain more generated code)
    existing_files.sort_by(|a, b| {
        // First sort by creation time (newer is better)
        let time_cmp = b.1.cmp(&a.1);
        if time_cmp == std::cmp::Ordering::Equal {
            // If same time, prefer larger files (more generated content)
            b.2.cmp(&a.2)
        } else {
            time_cmp
        }
    });

    eprintln!(
        "laz_client_macros: Found {} existing generated files:",
        existing_files.len()
    );
    for (path, _time, size) in &existing_files {
        eprintln!("  - {} ({} bytes)", path.display(), size);

        // Check if this is the full type-safe client or the fallback
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.contains("Auto-generated type-safe RPC client for server at:") {
                eprintln!("    -> This appears to be the full type-safe client!");
                return Ok(content);
            } else if content
                .contains("Runtime-generated RPC client (build-time generation failed)")
            {
                eprintln!("    -> This is the runtime fallback client");
            }
        }
    }

    // If we found existing files but none were the full client, use the newest one
    if let Some((path, _, _)) = existing_files.first() {
        eprintln!(
            "laz_client_macros: Using fallback client from: {}",
            path.display()
        );
        return fs::read_to_string(&path).map_err(|e| {
            format!(
                "Failed to read generated code from {}: {}",
                path.display(),
                e
            )
            .into()
        });
    }

    eprintln!("laz_client_macros: No generated code found in any of the expected locations");
    Err("No generated code found in any of the expected locations".into())
}

fn collect_target_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        let path = PathBuf::from(target_dir);
        if path.exists() {
            roots.push(path);
        }
    }

    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let manifest_path = PathBuf::from(manifest_dir);
        let manifest_target = manifest_path.join("target");
        if manifest_target.exists() {
            roots.push(manifest_target);
        }
        if let Some(parent) = manifest_path.parent() {
            let parent_target = parent.join("target");
            if parent_target.exists() {
                roots.push(parent_target);
            }
        }
    }

    if let Ok(cwd) = env::current_dir() {
        let cwd_target = cwd.join("target");
        if cwd_target.exists() {
            roots.push(cwd_target);
        }
        if let Some(parent) = cwd.parent() {
            let parent_target = parent.join("target");
            if parent_target.exists() {
                roots.push(parent_target);
            }
        }
    }

    roots
}

fn find_generated_file_in_target(target_root: &Path, profile: &str) -> Option<PathBuf> {
    let build_dir = target_root.join(profile).join("build");
    if !build_dir.exists() {
        return None;
    }

    let entries = std::fs::read_dir(build_dir).ok()?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path
            .file_name()
            .and_then(|f| f.to_str())
            .map(|name| name.starts_with("laz_client_macros-"))
            .unwrap_or(false)
        {
            let generated_file = entry_path.join("out").join("generated_rpc_client.rs");
            if generated_file.exists() {
                return Some(generated_file);
            }
        }
    }

    None
}

fn generate_runtime_fallback_client() -> String {
    r#"
/// Runtime-generated RPC client (build-time generation failed)
/// This client discovers functions dynamically at runtime
pub struct GeneratedRpcClient {
    inner: ::laz_client::LocoClient,
}

impl GeneratedRpcClient {
    /// Initialize the RPC client
    pub async fn init(server_addr: ::laz_client::ServerAddr) -> Result<Self, ::laz_client::RpcClientError> {
        let client = ::laz_client::LocoClient::init(server_addr).await?;
        Ok(Self { inner: client })
    }

    /// Get the underlying LocoClient for advanced usage
    pub fn inner(&self) -> &::laz_client::LocoClient {
        &self.inner
    }

    /// Get the server address
    pub fn server_addr(&self) -> &::laz_client::ServerAddr {
        &self.inner.server_addr
    }

    /// Call any RPC function by name with parameters
    pub async fn call(&self, function_name: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value, ::laz_client::RpcClientError> {
        self.inner.call_function(function_name, params).await
    }

    /// Get available function names from server metadata
    pub fn available_functions(&self) -> Vec<String> {
        self.inner.get_function_names()
    }
}
"#
    .to_string()
}
