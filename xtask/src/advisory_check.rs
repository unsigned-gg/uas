// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `cargo xtask advisory-check`: validate the `[advisories] ignore` entries of `deny.toml` (spec 0045 D2).
//!
//! `cargo xtask advisory-check [--today YYYY-MM-DD] [--config <path>] [--ids]`
//!
//! Each entry is a table `{ id, reason }` with `reason = "expires YYYY-MM-DD; <ticket>; <why>"`.
//! A bare id string, a malformed reason, a past expiry or an expiry more than 90 days after
//! `--today` fails. `--ids` prints the ids, one per line, without validating dates.

use std::path::Path;
use std::process::ExitCode;

/// Latest allowed expiry, in days after the day of the check.
const MAX_DAYS: i64 = 90;

/// A civil date as a day count since 1970-01-01.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date(i64);

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Parse `YYYY-MM-DD` as a real calendar date.
pub fn parse_date(s: &str) -> Option<Date> {
    let parts: Vec<&str> = s.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return None;
    };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    if ![y, m, d]
        .iter()
        .all(|p| p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let (y, m, d) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    Some(Date(days_from_civil(y, m, d)))
}

fn parse_table(text: &str) -> Result<toml::Table, String> {
    text.parse::<toml::Table>()
        .map_err(|e| format!("cannot parse: {e}"))
}

/// The `advisories.ignore` array, empty when the key is absent.
fn entries(table: &toml::Table) -> Result<&[toml::Value], String> {
    match table.get("advisories").and_then(|a| a.get("ignore")) {
        None => Ok(&[]),
        Some(toml::Value::Array(a)) => Ok(a),
        Some(_) => Err("advisories.ignore is not an array".to_string()),
    }
}

/// Every ignored id, in file order. A table without a string `id` is an error.
pub fn ids(text: &str) -> Result<Vec<String>, String> {
    let table = parse_table(text)?;
    entries(&table)?
        .iter()
        .map(|e| match e {
            toml::Value::String(s) => Ok(s.clone()),
            toml::Value::Table(t) => t
                .get("id")
                .and_then(toml::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| "an ignore entry has no string id".to_string()),
            _ => Err("an ignore entry is neither a string nor a table".to_string()),
        })
        .collect()
}

const SHAPE: &str = "reason must be `expires YYYY-MM-DD; <ticket>; <why>`";

/// The problem with one entry, if any.
fn entry_problem(entry: &toml::Table, today: Date) -> Option<String> {
    let Some(reason) = entry.get("reason").and_then(toml::Value::as_str) else {
        return Some("missing string reason".to_string());
    };
    let parts: Vec<&str> = reason.splitn(3, "; ").collect();
    let [first, ticket, why] = parts.as_slice() else {
        return Some(format!("{SHAPE}, got {reason:?}"));
    };
    if ticket.trim().is_empty() || why.trim().is_empty() {
        return Some(format!("{SHAPE}, got {reason:?}"));
    }
    let Some(expiry) = first.strip_prefix("expires ").and_then(parse_date) else {
        return Some(format!("{SHAPE} with a real date, got {reason:?}"));
    };
    if expiry < today {
        return Some(format!(
            "expired on {first:?}",
            first = &first["expires ".len()..]
        ));
    }
    if expiry.0 - today.0 > MAX_DAYS {
        return Some(format!("expiry is more than {MAX_DAYS} days after today"));
    }
    None
}

/// Problems with the ignore entries in `text`, one `<id>: <problem>` string each.
pub fn check(text: &str, today: Date, config: &Path) -> Vec<String> {
    let name = config.display();
    let table = match parse_table(text) {
        Ok(t) => t,
        Err(e) => return vec![format!("{name}: {e}")],
    };
    let list = match entries(&table) {
        Ok(l) => l,
        Err(e) => return vec![format!("{name}: {e}")],
    };
    let mut out = Vec::new();
    for e in list {
        match e {
            toml::Value::String(id) => {
                out.push(format!("{id}: bare id; use {{ id, reason }} with a reason"))
            }
            toml::Value::Table(t) => match t.get("id").and_then(toml::Value::as_str) {
                None => out.push(format!(
                    "entry has no string id: {}",
                    toml::Value::Table(t.clone())
                )),
                Some(id) => out.extend(entry_problem(t, today).map(|p| format!("{id}: {p}"))),
            },
            _ => out.push("?: entry is neither a string nor a table".to_string()),
        }
    }
    out
}

/// Today's UTC date from the system clock.
fn utc_today() -> Date {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Date(i64::try_from(secs / 86_400).unwrap_or(0))
}

const USAGE: &str =
    "usage: cargo xtask advisory-check [--today YYYY-MM-DD] [--config <path>] [--ids]";

pub fn run(root: &Path, args: &[String]) -> ExitCode {
    let mut today = None;
    let mut config = root.join("deny.toml");
    let mut list_ids = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match (a.as_str(), it.as_slice().first()) {
            ("--ids", _) => list_ids = true,
            ("--today", Some(v)) => {
                it.next();
                let Some(d) = parse_date(v) else {
                    eprintln!("advisory-check: --today {v:?} is not a YYYY-MM-DD date");
                    return ExitCode::from(1);
                };
                today = Some(d);
            }
            ("--config", Some(v)) => {
                it.next();
                config = Path::new(v).to_path_buf();
            }
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let text = match std::fs::read_to_string(&config) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("advisory-check: cannot read {}: {e}", config.display());
            return ExitCode::from(1);
        }
    };
    if list_ids {
        return match ids(&text) {
            Ok(v) => {
                for id in v {
                    println!("{id}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("advisory-check: {e}");
                ExitCode::from(1)
            }
        };
    }
    let problems = check(&text, today.unwrap_or_else(utc_today), &config);
    if problems.is_empty() {
        let n = ids(&text).map_or(0, |v| v.len());
        println!("advisory-check: ok ({n} ignore entries)");
        return ExitCode::SUCCESS;
    }
    for p in &problems {
        eprintln!("advisory-check: {p}");
    }
    ExitCode::from(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> Date {
        parse_date("2026-10-10").expect("date")
    }

    fn problems(fixture: &str) -> Vec<String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/advisory")
            .join(fixture);
        check(
            &std::fs::read_to_string(path).expect("fixture"),
            today(),
            Path::new("deny.toml"),
        )
    }

    #[test]
    fn advisory_valid_passes() {
        assert_eq!(problems("valid.toml"), Vec::<String>::new());
    }

    #[test]
    fn advisory_expired_fails() {
        let p = problems("expired.toml");
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(
            p[0].starts_with("RUSTSEC-2021-0139: ") && p[0].contains("expired"),
            "{p:?}"
        );
    }

    #[test]
    fn advisory_too_far_fails() {
        let p = problems("too-far.toml");
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("90 days"), "{p:?}");
    }

    #[test]
    fn advisory_bare_id_fails() {
        let p = problems("bare-id.toml");
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(
            p[0].starts_with("RUSTSEC-2021-0139: ") && p[0].contains("bare"),
            "{p:?}"
        );
    }

    #[test]
    fn advisory_no_ticket_fails() {
        let p = problems("no-ticket.toml");
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(
            p[0].contains("expires YYYY-MM-DD; <ticket>; <why>"),
            "{p:?}"
        );
    }

    #[test]
    fn advisory_missing_ignore_key_is_zero_entries() {
        assert_eq!(
            check(
                "[advisories]\nyanked = \"deny\"\n",
                today(),
                Path::new("deny.toml")
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            check("", today(), Path::new("deny.toml")),
            Vec::<String>::new()
        );
    }

    #[test]
    fn advisory_expiry_equal_to_today_passes_and_bad_calendar_date_fails() {
        let on = "[advisories]\nignore = [{ id = \"X\", reason = \"expires 2026-10-10; CER-1; why\" }]\n";
        assert_eq!(
            check(on, today(), Path::new("deny.toml")),
            Vec::<String>::new()
        );
        let bad = on.replace("2026-10-10", "2027-02-29");
        let p = check(&bad, today(), Path::new("deny.toml"));
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("real date"), "{p:?}");
    }

    #[test]
    fn advisory_ids_lists_every_id_without_validating() {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/advisory/expired.toml"),
        )
        .expect("fixture");
        assert_eq!(
            ids(&text).expect("parses"),
            vec!["RUSTSEC-2021-0139".to_string()]
        );
        let bare = "[advisories]\nignore = [\"A\", { id = \"B\", reason = \"r\" }]\n";
        assert_eq!(
            ids(bare).expect("parses"),
            vec!["A".to_string(), "B".to_string()]
        );
    }
}
