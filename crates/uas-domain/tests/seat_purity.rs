// SPDX-License-Identifier: Apache-2.0 OR MIT
//! REQ-0058-10: the seat types do no I/O.

const SEAT_SOURCE: &str = include_str!("../src/seat.rs");

#[test]
fn seat_types_use_no_io_apis() {
    // Strip the unit tests: they may use anything. Scan the code above them.
    let code = SEAT_SOURCE
        .split("#[cfg(test)]")
        .next()
        .expect("split yields one part");
    for banned in ["std::fs", "std::net", "std::process", "tokio"] {
        assert!(!code.contains(banned), "seat.rs uses {banned}");
    }
}
