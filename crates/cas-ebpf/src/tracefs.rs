//! Reads the sched tracepoint field offsets from tracefs at load (spec 0014 D13, T16).
//!
//! The field offsets differ by kernel. On 6.6 `child_pid` sits at 44; on 6.18 it sits
//! at 20 (T16). The loader reads the three `format` files, checks the five fields the
//! kernel program uses, and writes their offsets into `TP_OFFSETS` before any attach.
//! Nothing here touches the kernel: [`parse`] takes the file text, so it runs unprivileged.

use std::{fs, io};

use thiserror::Error;

/// The three sched tracepoints the loader reads, as `category/name`.
const EXEC: &str = "sched/sched_process_exec";
/// See [`EXEC`].
const FORK: &str = "sched/sched_process_fork";
/// See [`EXEC`].
const EXIT: &str = "sched/sched_process_exit";
/// Tracefs roots, primary first. The debugfs mount is the fallback.
const ROOTS: [&str; 2] = ["/sys/kernel/tracing", "/sys/kernel/debug/tracing"];

/// The five field offsets the kernel program reads (spec 0014 D13).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TpOffsets {
    /// `sched_process_exec` `pid`.
    pub exec_pid: u32,
    /// `sched_process_exec` `filename`, a `__data_loc` word.
    pub exec_filename: u32,
    /// `sched_process_fork` `parent_pid`.
    pub fork_parent_pid: u32,
    /// `sched_process_fork` `child_pid`.
    pub fork_child_pid: u32,
    /// `sched_process_exit` `pid`.
    pub exit_pid: u32,
}

impl TpOffsets {
    /// The offsets in `TP_OFFSETS` slot order: exec pid, exec filename, fork
    /// parent_pid, fork child_pid, exit pid (spec 0014 D13).
    pub fn slots(&self) -> [u32; 5] {
        [
            self.exec_pid,
            self.exec_filename,
            self.fork_parent_pid,
            self.fork_child_pid,
            self.exit_pid,
        ]
    }
}

/// Why one `format` file was refused. Each variant names the field it concerns.
#[derive(Debug, Error)]
pub enum FormatError {
    /// No tracefs root has the file, or it could not be read.
    #[error("format file unreadable at {path}: {source}")]
    Unreadable {
        /// The path of the primary root, which is tried first.
        path: String,
        /// The I/O error from the primary root.
        #[source]
        source: io::Error,
    },
    /// The format has no such field, or its offset or size line is malformed.
    #[error("field {field} is missing or malformed")]
    FieldMissing {
        /// The field looked up.
        field: &'static str,
    },
    /// The field's declared type is not the one the probe reads.
    #[error("field {field} has type `{found}`")]
    WrongType {
        /// The field looked up.
        field: &'static str,
        /// The declared type, without the field name.
        found: String,
    },
    /// The field is not 4 bytes wide.
    #[error("field {field} has size {size}, want 4")]
    WrongSize {
        /// The field looked up.
        field: &'static str,
        /// The declared size.
        size: u32,
    },
    /// The offset is below 8, or offset plus size is above 256.
    #[error("field {field} at offset {offset} with size {size} is out of bounds")]
    OffsetOutOfRange {
        /// The field looked up.
        field: &'static str,
        /// The declared offset.
        offset: u32,
        /// The declared size.
        size: u32,
    },
}

/// A refused tracepoint: which `format` file, and why.
#[derive(Debug, Error)]
#[error("tracepoint {tracepoint}: {source}")]
pub struct FormatFailure {
    /// The tracepoint, as `category/name`.
    pub tracepoint: &'static str,
    /// What was wrong with its `format` file.
    #[source]
    pub source: FormatError,
}

/// Reads the three `format` files from tracefs and parses them.
///
/// # Errors
///
/// [`FormatFailure`] for the first tracepoint whose file is unreadable or breaks a
/// D13 rule. The caller attaches nothing when this fails.
pub fn read() -> Result<TpOffsets, FormatFailure> {
    let exec = format_text(EXEC)?;
    let fork = format_text(FORK)?;
    let exit = format_text(EXIT)?;
    parse(&exec, &fork, &exit)
}

/// Reads one `format` file, trying each tracefs root in turn.
fn format_text(tracepoint: &'static str) -> Result<String, FormatFailure> {
    let mut primary: Option<FormatError> = None;
    for root in ROOTS {
        let path = format!("{root}/events/{tracepoint}/format");
        match fs::read_to_string(&path) {
            Ok(text) => return Ok(text),
            Err(source) => {
                if primary.is_none() {
                    primary = Some(FormatError::Unreadable { path, source });
                }
            }
        }
    }
    let source = primary.unwrap_or(FormatError::FieldMissing { field: "format" });
    Err(FormatFailure { tracepoint, source })
}

/// Parses the three `format` texts (exec, fork, exit) into the five offsets.
///
/// # Errors
///
/// [`FormatFailure`] naming the tracepoint and the rule the field broke.
pub fn parse(exec: &str, fork: &str, exit: &str) -> Result<TpOffsets, FormatFailure> {
    let fail = |tracepoint: &'static str| move |source| FormatFailure { tracepoint, source };
    Ok(TpOffsets {
        exec_pid: pid_offset(exec, "pid").map_err(fail(EXEC))?,
        exec_filename: data_loc_offset(exec, "filename").map_err(fail(EXEC))?,
        fork_parent_pid: pid_offset(fork, "parent_pid").map_err(fail(FORK))?,
        fork_child_pid: pid_offset(fork, "child_pid").map_err(fail(FORK))?,
        exit_pid: pid_offset(exit, "pid").map_err(fail(EXIT))?,
    })
}

/// One `field:` line of a format file, after the name is matched.
struct Field {
    ty: String,
    size: u32,
    offset: u32,
}

/// Finds the line whose last declarator name is `name`. The match is exact, so
/// `pid` does not match `common_pid` or `old_pid`. Array suffixes are dropped.
fn find(text: &str, name: &'static str) -> Result<Field, FormatError> {
    for line in text.lines() {
        let Some(body) = line.trim_start().strip_prefix("field:") else {
            continue;
        };
        let mut parts = body.split(';');
        let decl = parts.next().unwrap_or_default().trim();
        let Some((ty, last)) = decl.rsplit_once(char::is_whitespace) else {
            continue;
        };
        let field_name = last.split('[').next().unwrap_or(last);
        if field_name != name {
            continue;
        }
        let mut offset = None;
        let mut size = None;
        for part in parts {
            let part = part.trim();
            if let Some(value) = part.strip_prefix("offset:") {
                offset = value.parse::<u32>().ok();
            } else if let Some(value) = part.strip_prefix("size:") {
                size = value.parse::<u32>().ok();
            }
        }
        return match (offset, size) {
            (Some(offset), Some(size)) => Ok(Field {
                ty: ty.trim().to_owned(),
                size,
                offset,
            }),
            _ => Err(FormatError::FieldMissing { field: name }),
        };
    }
    Err(FormatError::FieldMissing { field: name })
}

/// Offset of a `pid_t` field of size 4.
fn pid_offset(text: &str, field: &'static str) -> Result<u32, FormatError> {
    let f = find(text, field)?;
    if f.ty != "pid_t" {
        return Err(FormatError::WrongType { field, found: f.ty });
    }
    bounds(field, &f)
}

/// Offset of a literal `__data_loc` field of size 4. `__rel_loc` is refused.
fn data_loc_offset(text: &str, field: &'static str) -> Result<u32, FormatError> {
    let f = find(text, field)?;
    // The first whitespace token must be exactly `__data_loc`, so `__data_locx` is refused.
    if f.ty.split_whitespace().next() != Some("__data_loc") {
        return Err(FormatError::WrongType { field, found: f.ty });
    }
    bounds(field, &f)
}

/// The size and bound rules: size 4, offset at least 8, offset plus size at most 256.
fn bounds(field: &'static str, f: &Field) -> Result<u32, FormatError> {
    if f.size != 4 {
        return Err(FormatError::WrongSize {
            field,
            size: f.size,
        });
    }
    match f.offset.checked_add(f.size) {
        Some(end) if f.offset >= 8 && end <= 256 => Ok(f.offset),
        _ => Err(FormatError::OffsetOutOfRange {
            field,
            offset: f.offset,
            size: f.size,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured 2026-10-10 from the ci-kernel node, kernel 6.18.51-talos: the
    // sched format files, job 114140426567 of run 38027192645. Tabs are real.
    const EXEC_618: &str = r#"name: sched_process_exec
ID: 405
format:
	field:unsigned short common_type;	offset:0;	size:2;	signed:0;
	field:unsigned char common_flags;	offset:2;	size:1;	signed:0;
	field:unsigned char common_preempt_count;	offset:3;	size:1;	signed:0;
	field:int common_pid;	offset:4;	size:4;	signed:1;

	field:__data_loc char[] filename;	offset:8;	size:4;	signed:0;
	field:pid_t pid;	offset:12;	size:4;	signed:1;
	field:pid_t old_pid;	offset:16;	size:4;	signed:1;

print fmt: "filename=%s pid=%d old_pid=%d", __get_str(filename), REC->pid, REC->old_pid
"#;
    const FORK_618: &str = r#"name: sched_process_fork
ID: 404
format:
	field:unsigned short common_type;	offset:0;	size:2;	signed:0;
	field:unsigned char common_flags;	offset:2;	size:1;	signed:0;
	field:unsigned char common_preempt_count;	offset:3;	size:1;	signed:0;
	field:int common_pid;	offset:4;	size:4;	signed:1;

	field:__data_loc char[] parent_comm;	offset:8;	size:4;	signed:0;
	field:pid_t parent_pid;	offset:12;	size:4;	signed:1;
	field:__data_loc char[] child_comm;	offset:16;	size:4;	signed:0;
	field:pid_t child_pid;	offset:20;	size:4;	signed:1;

print fmt: "comm=%s pid=%d child_comm=%s child_pid=%d", __get_str(parent_comm), REC->parent_pid, __get_str(child_comm), REC->child_pid
"#;
    const EXIT_618: &str = r#"name: sched_process_exit
ID: 401
format:
	field:unsigned short common_type;	offset:0;	size:2;	signed:0;
	field:unsigned char common_flags;	offset:2;	size:1;	signed:0;
	field:unsigned char common_preempt_count;	offset:3;	size:1;	signed:0;
	field:int common_pid;	offset:4;	size:4;	signed:1;

	field:char comm[16];	offset:8;	size:16;	signed:0;
	field:pid_t pid;	offset:24;	size:4;	signed:1;
	field:int prio;	offset:28;	size:4;	signed:1;
	field:bool group_dead;	offset:32;	size:1;	signed:0;

print fmt: "comm=%s pid=%d prio=%d group_dead=%s", REC->comm, REC->pid, REC->prio, REC->group_dead ? "true" : "false"
"#;
    const EXEC_66: &str = r#"name: sched_process_exec
format:
	field:unsigned short common_type;	offset:0;	size:2;	signed:0;
	field:unsigned char common_flags;	offset:2;	size:1;	signed:0;
	field:unsigned char common_preempt_count;	offset:3;	size:1;	signed:0;
	field:int common_pid;	offset:4;	size:4;	signed:1;

	field:__data_loc char[] filename;	offset:8;	size:4;	signed:0;
	field:pid_t pid;	offset:12;	size:4;	signed:1;
	field:pid_t old_pid;	offset:16;	size:4;	signed:1;

print fmt: "filename=%s pid=%d old_pid=%d", __get_str(filename), REC->pid, REC->old_pid
"#;
    const FORK_66: &str = r#"name: sched_process_fork
format:
	field:unsigned short common_type;	offset:0;	size:2;	signed:0;
	field:unsigned char common_flags;	offset:2;	size:1;	signed:0;
	field:unsigned char common_preempt_count;	offset:3;	size:1;	signed:0;
	field:int common_pid;	offset:4;	size:4;	signed:1;

	field:char parent_comm[16];	offset:8;	size:16;	signed:0;
	field:pid_t parent_pid;	offset:24;	size:4;	signed:1;
	field:char child_comm[16];	offset:28;	size:16;	signed:0;
	field:pid_t child_pid;	offset:44;	size:4;	signed:1;

print fmt: "comm=%s pid=%d child_comm=%s child_pid=%d", REC->parent_comm, REC->parent_pid, REC->child_comm, REC->child_pid
"#;
    const EXIT_66: &str = r#"name: sched_process_exit
format:
	field:unsigned short common_type;	offset:0;	size:2;	signed:0;
	field:unsigned char common_flags;	offset:2;	size:1;	signed:0;
	field:unsigned char common_preempt_count;	offset:3;	size:1;	signed:0;
	field:int common_pid;	offset:4;	size:4;	signed:1;

	field:char comm[16];	offset:8;	size:16;	signed:0;
	field:pid_t pid;	offset:24;	size:4;	signed:1;
	field:int prio;	offset:28;	size:4;	signed:1;

print fmt: "comm=%s pid=%d prio=%d", REC->comm, REC->pid, REC->prio
"#;

    // Built from the spec 0014 T7 ceres 6.6 offsets, in the same shape as the node
    // capture. It is not a captured file: the T7 offsets are the only source.
    // Used to pin the 6.6 slots, which must stay correct after the change.

    #[test]
    fn node_6_18_offsets_are_read_from_the_format() -> Result<(), FormatFailure> {
        assert_eq!(
            parse(EXEC_618, FORK_618, EXIT_618)?.slots(),
            [12, 8, 12, 20, 24]
        );
        Ok(())
    }

    #[test]
    fn ceres_6_6_offsets_are_read_from_the_format() -> Result<(), FormatFailure> {
        assert_eq!(
            parse(EXEC_66, FORK_66, EXIT_66)?.slots(),
            [12, 8, 24, 44, 24]
        );
        Ok(())
    }

    #[test]
    fn rel_loc_filename_is_refused() {
        let exec = EXEC_618.replace("__data_loc char[] filename", "__rel_loc char[] filename");
        assert!(matches!(
            parse(&exec, FORK_618, EXIT_618),
            Err(FormatFailure {
                tracepoint: EXEC,
                source: FormatError::WrongType {
                    field: "filename",
                    ..
                },
            })
        ));
    }

    #[test]
    fn pid_of_size_8_is_refused() {
        let exec = EXEC_618.replace(
            "pid_t pid;\toffset:12;\tsize:4;",
            "pid_t pid;\toffset:12;\tsize:8;",
        );
        assert!(matches!(
            parse(&exec, FORK_618, EXIT_618),
            Err(FormatFailure {
                tracepoint: EXEC,
                source: FormatError::WrongSize {
                    field: "pid",
                    size: 8
                },
            })
        ));
    }

    #[test]
    fn offset_254_with_size_4_is_refused() {
        let exit = EXIT_618.replace(
            "pid_t pid;\toffset:24;\tsize:4;",
            "pid_t pid;\toffset:254;\tsize:4;",
        );
        assert!(matches!(
            parse(EXEC_618, FORK_618, &exit),
            Err(FormatFailure {
                tracepoint: EXIT,
                source: FormatError::OffsetOutOfRange {
                    field: "pid",
                    offset: 254,
                    size: 4
                },
            })
        ));
    }

    #[test]
    fn offset_4_is_refused() {
        let exec = EXEC_618.replace("pid_t pid;\toffset:12;", "pid_t pid;\toffset:4;");
        assert!(matches!(
            parse(&exec, FORK_618, EXIT_618),
            Err(FormatFailure {
                tracepoint: EXEC,
                source: FormatError::OffsetOutOfRange {
                    field: "pid",
                    offset: 4,
                    size: 4
                },
            })
        ));
    }

    #[test]
    fn offset_252_with_size_4_is_accepted() -> Result<(), FormatFailure> {
        let exit = EXIT_618.replace("pid_t pid;\toffset:24;", "pid_t pid;\toffset:252;");
        assert_eq!(parse(EXEC_618, FORK_618, &exit)?.exit_pid, 252);
        Ok(())
    }

    #[test]
    fn offset_253_with_size_4_is_refused() {
        let exit = EXIT_618.replace("pid_t pid;\toffset:24;", "pid_t pid;\toffset:253;");
        assert!(matches!(
            parse(EXEC_618, FORK_618, &exit),
            Err(FormatFailure {
                tracepoint: EXIT,
                source: FormatError::OffsetOutOfRange {
                    field: "pid",
                    offset: 253,
                    size: 4
                },
            })
        ));
    }

    #[test]
    fn int_pid_is_refused() {
        let exec = EXEC_618.replace("pid_t pid;\toffset:12;", "int pid;\toffset:12;");
        assert!(matches!(
            parse(&exec, FORK_618, EXIT_618),
            Err(FormatFailure {
                tracepoint: EXEC,
                source: FormatError::WrongType { field: "pid", .. },
            })
        ));
    }

    #[test]
    fn data_locx_filename_is_refused() -> Result<(), FormatFailure> {
        let exec = EXEC_618.replace("__data_loc char[] filename", "__data_locx char[] filename");
        assert!(matches!(
            parse(&exec, FORK_618, EXIT_618),
            Err(FormatFailure {
                tracepoint: EXEC,
                source: FormatError::WrongType {
                    field: "filename",
                    ..
                },
            })
        ));
        // The real line still parses.
        parse(EXEC_618, FORK_618, EXIT_618)?;
        Ok(())
    }

    #[test]
    fn fork_without_child_pid_is_refused() {
        let fork: String = FORK_618
            .lines()
            .filter(|line| !line.contains("field:pid_t child_pid"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(matches!(
            parse(EXEC_618, &fork, EXIT_618),
            Err(FormatFailure {
                tracepoint: FORK,
                source: FormatError::FieldMissing { field: "child_pid" },
            })
        ));
    }
}
