use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[path = "codegen_shared.rs"]
mod codegen_shared;

use codegen_shared::generate_client_code_from_server;

fn main() {
    setup_rerun_triggers();

    let server_url =
        env::var("LAZ_SERVER_URL").unwrap_or_else(|_| "http://localhost:5150".to_string());
    println!(
        "cargo:warning=Generating type-safe RPC client for server: {}",
        server_url
    );

    match generate_client_code_from_server(&server_url) {
        Ok((generated_code, metadata_json)) => {
            write_generated_client(&generated_code);
            if record_metadata_cache(&metadata_json) {
                println!("cargo:warning=Server metadata changed, forcing regeneration");
            }
        }
        Err(e) => {
            println!(
                "cargo:warning=Failed to generate type-safe RPC client code: {}",
                e
            );
            println!(
                "cargo:warning=This is expected during initial build when server is not running"
            );
            println!("cargo:warning=The client will use a basic implementation");
            write_generated_client(get_basic_runtime_client_code());
        }
    }
}

fn write_generated_client(code: &str) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_rpc_client.rs");

    let mut f = File::create(&dest_path).unwrap();
    writeln!(f, "{}", code).unwrap();

    println!(
        "cargo:warning=Generated RPC client code written to: {}",
        dest_path.display()
    );
    println!(
        "cargo:rustc-env=LAZ_CLIENT_GENERATED_CODE_PATH={}",
        dest_path.display()
    );
    println!("cargo:rerun-if-changed={}", dest_path.display());
}

fn setup_rerun_triggers() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LAZ_SERVER_URL");

    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let manifest_dir = PathBuf::from(manifest_dir);
        if let Some(workspace_root) = manifest_dir.parent().and_then(|p| p.parent()) {
            let watch_paths = [
                workspace_root.join("server/src"),
                workspace_root.join("server/routes"),
                workspace_root.join("server/views"),
                workspace_root.join("server/config"),
            ];

            for path in &watch_paths {
                watch_path_recursively(path);
            }
        } else {
            println!("cargo:warning=Unable to locate workspace root for change tracking");
        }
    } else {
        println!("cargo:warning=CARGO_MANIFEST_DIR not set; change tracking limited");
    }
}

fn watch_path_recursively(path: &Path) {
    if !path.exists() {
        return;
    }

    let Ok(metadata) = fs::symlink_metadata(path) else {
        println!(
            "cargo:warning=Failed to access path for change tracking: {}",
            path.display()
        );
        return;
    };

    if metadata.file_type().is_symlink() {
        return;
    }

    println!("cargo:rerun-if-changed={}", path.display());

    if metadata.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                watch_path_recursively(&entry.path());
            }
        }
    }
}

fn record_metadata_cache(metadata_json: &str) -> bool {
    let cache_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("metadata_cache.json");
    let mut changed = true;

    if cache_path.exists() {
        if let Ok(existing) = fs::read_to_string(&cache_path) {
            if existing == metadata_json {
                changed = false;
            }
        }
    }

    if let Ok(mut f) = File::create(&cache_path) {
        let _ = f.write_all(metadata_json.as_bytes());
    }

    changed
}

fn get_basic_runtime_client_code() -> &'static str {
    r#"
/// Runtime-generated RPC client (build-time generation failed)
/// This client discovers functions dynamically at runtime
pub struct RpcClient {
    inner: ::laz_client::LocoClient,
}

impl RpcClient {
    pub async fn init(server_addr: ::laz_client::ServerAddr) -> Result<Self, ::laz_client::RpcClientError> {
        let client = ::laz_client::LocoClient::init(server_addr).await?;
        Ok(Self { inner: client })
    }

    pub fn inner(&self) -> &::laz_client::LocoClient {
        &self.inner
    }

    pub fn server_addr(&self) -> &::laz_client::ServerAddr {
        &self.inner.server_addr
    }

    pub async fn call(&self, function_name: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value, ::laz_client::RpcClientError> {
        self.inner.call_function(function_name, params).await
    }
    
    pub fn available_functions(&self) -> Vec<String> {
        self.inner.get_function_names()
    }
}

pub use RpcClient as GeneratedRpcClient;
"#
}
