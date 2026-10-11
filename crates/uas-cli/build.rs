//! Build-time version stamp. The logic lives in `build-support/build_rev.rs`.

include!("../../build-support/build_rev.rs");

fn main() {
    emit_build_rev();
}
