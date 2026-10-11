// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Compiles the vendored Cortex pool protos for the feature `cortex-grpc` (spec 0026 P1). With
//! the feature off it does nothing, so a default build needs neither `protox` nor `tonic`.

#[cfg(feature = "cortex-grpc")]
fn generate() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, path::PathBuf};

    let out = PathBuf::from(env::var("OUT_DIR")?);
    let vendored = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("../../vendor/cortex/proto");
    // The vendored files are flat; their imports name `cortex/mesh/v1/` (spec 0026 F7).
    let include = out.join("proto");
    let dest = include.join("cortex/mesh/v1");
    fs::create_dir_all(&dest)?;
    for name in ["pool.proto", "lane.proto", "peer.proto"] {
        let from = vendored.join(name);
        println!("cargo:rerun-if-changed={}", from.display());
        fs::copy(&from, dest.join(name))?;
    }
    let descriptors = protox::compile(["cortex/mesh/v1/pool.proto"], [&include])?;
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_fds(descriptors)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_FEATURE_CORTEX_GRPC").is_none() {
        return Ok(());
    }
    #[cfg(feature = "cortex-grpc")]
    generate()?;
    Ok(())
}
