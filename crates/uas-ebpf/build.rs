//! Checks the committed BPF object against its recorded SHA-256 (spec 0014 D3).
//!
//! The kernel program is built outside the workspace with nightly and
//! `bpf-linker` (`make ebpf-build`). CI has neither, so the object is
//! committed. This script refuses to build the crate when the object and
//! `ebpf/uas-ebpf.o.sha256` disagree, so a stale or hand-edited object never
//! ships inside the loader.

use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf, process};

fn main() {
    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        fail("CARGO_MANIFEST_DIR is unset");
    };
    let root = PathBuf::from(manifest_dir);
    let object = root.join("ebpf/uas-ebpf.o");
    let recorded = root.join("ebpf/uas-ebpf.o.sha256");
    println!("cargo:rerun-if-changed={}", object.display());
    println!("cargo:rerun-if-changed={}", recorded.display());

    let bytes = match fs::read(&object) {
        Ok(bytes) => bytes,
        Err(err) => fail(&format!("cannot read {}: {err}", object.display())),
    };
    let want = match fs::read_to_string(&recorded) {
        Ok(text) => text.split_whitespace().next().unwrap_or("").to_owned(),
        Err(err) => fail(&format!("cannot read {}: {err}", recorded.display())),
    };
    let got = hex(&Sha256::digest(&bytes));
    if got != want {
        fail(&format!(
            "ebpf/uas-ebpf.o sha256 {got} does not match ebpf/uas-ebpf.o.sha256 ({want}). \
             Run `make ebpf-build` and commit both files."
        ));
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn fail(message: &str) -> ! {
    eprintln!("uas-ebpf build.rs: {message}");
    process::exit(1)
}
