// SPDX-License-Identifier: Apache-2.0 OR MIT
//! REQ-0058-10: the seat types and the plan types do no I/O.

const SEAT_SOURCES: [(&str, &str); 2] = [
    ("seat.rs", include_str!("../src/seat.rs")),
    ("seat_plan.rs", include_str!("../src/seat_plan.rs")),
];

#[test]
fn seat_types_use_no_io_apis() {
    for (name, source) in SEAT_SOURCES {
        // Strip the unit tests: they may use anything. Scan the code above them.
        let code = source
            .split("#[cfg(test)]")
            .next()
            .expect("split yields one part");
        for banned in ["std::fs", "std::net", "std::process", "tokio"] {
            assert!(!code.contains(banned), "{name} uses {banned}");
        }
    }
}
