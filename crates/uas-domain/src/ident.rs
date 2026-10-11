// SPDX-License-Identifier: Apache-2.0 OR MIT
use crate::DomainError;

/// Check `text` against the rules for one kind of identifier.
///
/// The text must be 1 to `max` bytes, start with a lowercase letter, and
/// contain only `a-z`, `0-9`, `-` and the characters in `extra`.
pub(crate) fn check(
    kind: &'static str,
    text: &str,
    max: usize,
    extra: &str,
) -> Result<(), DomainError> {
    let reason = if text.is_empty() {
        Some("must not be empty")
    } else if text.len() > max {
        Some("is too long")
    } else if !text.starts_with(|c: char| c.is_ascii_lowercase()) {
        Some("must start with a lowercase letter")
    } else if !text
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || extra.contains(c))
    {
        Some("contains a character that is not allowed")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(DomainError::Invalid {
            kind,
            value: text.to_owned(),
            reason,
        }),
        None => Ok(()),
    }
}
