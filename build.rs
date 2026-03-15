//! Build script for nix-module-system
//!
//! This handles:
//! 1. Finding Nix libraries via pkg-config (optional)
//! 2. Compiling C++ FFI code (optional)
//! 3. Setting up version-specific compatibility

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/ffi/plugin.cpp");
    println!("cargo:rerun-if-changed=src/ffi/compat.h");

    // Check if nix-ffi feature is enabled
    let nix_ffi_enabled = env::var("CARGO_FEATURE_NIX_FFI").is_ok();

    if !nix_ffi_enabled {
        // Skip FFI compilation if feature not enabled
        println!("cargo:warning=nix-ffi feature not enabled, skipping Nix C API integration");
        return;
    }

    // Find Nix libraries
    let nix_expr = match pkg_config::Config::new()
        .atleast_version("2.18.0")
        .probe("nix-expr-c")
    {
        Ok(lib) => lib,
        Err(e) => {
            println!("cargo:warning=nix-expr-c not found: {}", e);
            println!(
                "cargo:warning=Set PKG_CONFIG_PATH or install nix.dev to enable FFI integration"
            );
            return;
        }
    };

    let nix_store = match pkg_config::Config::new()
        .atleast_version("2.18.0")
        .probe("nix-store-c")
    {
        Ok(lib) => lib,
        Err(e) => {
            println!("cargo:warning=nix-store-c not found: {}", e);
            return;
        }
    };

    // Parse Nix version for compatibility
    let version = &nix_expr.version;
    let parts: Vec<u32> = version
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();

    // Check if C++ plugin.cpp exists
    let plugin_cpp = PathBuf::from("src/ffi/plugin.cpp");
    if plugin_cpp.exists() {
        let mut build = cc::Build::new();
        build
            .cpp(true)
            .std("c++20")
            .opt_level(2)
            .file("src/ffi/plugin.cpp")
            .include("src/ffi");

        // Add include paths from pkg-config
        for path in nix_expr
            .include_paths
            .iter()
            .chain(nix_store.include_paths.iter())
        {
            build.include(path);
        }

        // Version-specific defines
        if parts.len() >= 2 {
            let (major, minor) = (parts[0], parts[1]);

            // Nix 2.18+ has stable C API
            if major >= 2 && minor >= 18 {
                build.define("NIX_2_18_0", None);
            }

            // Nix 2.20+ has additional features
            if major >= 2 && minor >= 20 {
                build.define("NIX_2_20_0", None);
            }
        }

        build.compile("nix_module_system_cpp");
    }

    // Output directory for generated files
    let out_dir = env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-env=OUT_DIR={}", out_dir);
}
