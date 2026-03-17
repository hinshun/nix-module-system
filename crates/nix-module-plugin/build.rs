//! Build script for nix-module-plugin.
//!
//! Detects Nix C API libraries via pkg-config and sets up linker flags.
//! The plugin is a cdylib loaded into the Nix process, so we need the
//! Nix C API headers for type declarations (symbols resolve at load time).

fn main() {
    // Find Nix C API libraries for linker flags
    match pkg_config::Config::new()
        .atleast_version("2.18.0")
        .probe("nix-expr-c")
    {
        Ok(lib) => {
            // Parse version for conditional compilation
            let parts: Vec<u32> = lib
                .version
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();

            if parts.len() >= 2 {
                let (major, minor) = (parts[0], parts[1]);
                if major >= 2 && minor >= 18 {
                    println!("cargo:rustc-cfg=nix_2_18");
                }
                if major >= 2 && minor >= 20 {
                    println!("cargo:rustc-cfg=nix_2_20");
                }
            }

            println!("cargo:rustc-env=NIX_VERSION={}", lib.version);
        }
        Err(e) => {
            println!("cargo:warning=nix-expr-c not found: {}", e);
            println!("cargo:warning=Plugin will compile but may not link correctly");
            println!("cargo:warning=Set PKG_CONFIG_PATH or install nix.dev");
        }
    }

    // Also probe nix-store-c (often needed transitively)
    if let Err(e) = pkg_config::Config::new()
        .atleast_version("2.18.0")
        .probe("nix-store-c")
    {
        println!("cargo:warning=nix-store-c not found: {}", e);
    }
}
