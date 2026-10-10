//! Saga flows for the cas-cpm waves, and the `cas wave step` shim (spec 0025 D2).
//!
//! `cas wave flow` writes one saga flow per wave. Each step of a flow runs `cas wave step` as its
//! `verify_cmd`, so saga owns the run and the gates, and the executor owns the custody calls.
//! `cas wave step` runs one step through `WaveExecutor` and maps the outcome to an exit code.
//!
//! The pure functions here take their environment as arguments, so the tests include this file
//! and call them without a subprocess.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use cas_engine::{
    AgentRole, CompensationReceipt, FlowMetadata, HaltReason, SagaFlowDef, StepOutcome, WaveError,
    WaveExecConfig, WaveExecutor, WavePlan, WaveStep,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The file name prefix and flow name prefix of a generated wave.
const WAVE_PREFIX: &str = "cas-cpm-wave-w";
/// The prompt of every generated step: the step runs `verify_cmd` and nothing else (spec 0025 D2).
const SHIM_PROMPT: &str = "Do nothing. The verify_cmd runs this step. Report its exit.";
/// The approval gate of a MainSession step, unless the plan names another gate.
const DEFAULT_GATE: &str = "heimdal:approval";
/// The saga agent of every shim step. Saga fails a step with no agent as "unassigned agent".
const SHIM_AGENT: &str = "auto";
/// The prompt of a gate step. Saga requires a prompt on every step, so a gate has one too.
const GATE_PROMPT: &str = "Operator gate: a person approves or denies this step in saga. Read the results of the steps before it first.";

/// The `[limits]` table of a generated flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Limits {
    /// The plan's `max_concurrency`.
    pub max_in_flight: usize,
}

/// One step of a generated flow. A shim step names `agent = "auto"`. A gate names no agent,
/// as `.saga/flows/gates.toml` does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenStep {
    /// The step id, the same as the plan's.
    pub id: String,
    /// The step title, the same as the plan's.
    pub title: String,
    /// `impl` for a Builder, `review` for a Critic, `test` for a Probe, `gate` for a MainSession.
    pub kind: &'static str,
    /// The saga agent: `auto` on a shim step, absent on a gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<&'static str>,
    /// The dependencies that lie in the same file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<String>,
    /// The literal argv that runs the step. Empty for a gate.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub verify_cmd: Vec<String>,
    /// The agent prompt. Absent for a gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// The gate name. Set for a gate only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
}

/// One wave as a saga flow: the shape `SagaFlowDef` parses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedFlow {
    /// The flow header.
    pub flow: FlowMetadata,
    /// The concurrency limit of the wave.
    pub limits: Limits,
    /// The steps of the wave, in plan order, gates first.
    #[serde(rename = "step")]
    pub step: Vec<GenStep>,
}

/// One generated file: its name and its flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaveFile {
    /// The file name, `cas-cpm-wave-w<i>.toml`.
    pub name: String,
    /// The flow that the file holds.
    pub flow: GeneratedFlow,
}

/// The argv of a shim step. `requested` is the `--max-concurrency` of `wave flow`, and it is
/// repeated here so that `wave step` plans the waves the same way (0 adds nothing).
fn shim_argv(source: &str, step: &str, requested: usize) -> Vec<String> {
    let mut argv: Vec<String> = ["cas", "wave", "step", "--flow", source, "--step", step]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    if requested != 0 {
        argv.push("--max-concurrency".into());
        argv.push(requested.to_string());
    }
    argv
}

/// The default `merge_wait`, in seconds (spec 0025 D15).
const DEFAULT_MERGE_WAIT_SECS: u64 = 900;

/// The argv of a merge step: `cas wave merge-gate` under an outer `timeout`, because saga bounds
/// no `verify_cmd` (spec 0025 F22). `outer` is `merge_wait + 60` seconds.
fn merge_argv(source: &str, builder: &str, outer: u64, requested: usize) -> Vec<String> {
    let mut argv: Vec<String> = [
        "timeout",
        "-k",
        "30",
        &outer.to_string(),
        "cas",
        "wave",
        "merge-gate",
        "--flow",
        source,
        "--builder",
        builder,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if requested != 0 {
        argv.push("--max-concurrency".into());
        argv.push(requested.to_string());
    }
    argv
}

/// Build one flow per wave of `plan`. The output is deterministic: it depends only on the plan
/// and the arguments. `requested` is the `--max-concurrency` that the command received.
///
/// A wave with no step left writes no flow, for example a wave whose only step is a gate that
/// moved to its dependent's wave. Each file keeps the plan's wave index in its name.
pub fn generate(
    plan: &WavePlan,
    source: &str,
    requested: usize,
    merge: &PlanMerge,
) -> Vec<WaveFile> {
    let all: Vec<(usize, &WaveStep)> = plan
        .waves
        .iter()
        .enumerate()
        .flat_map(|(i, w)| w.steps.iter().map(move |s| (i, s)))
        .collect();
    let roles: BTreeMap<&str, AgentRole> = all
        .iter()
        .map(|(_, s)| (s.step.id.as_str(), s.role))
        .collect();

    // The waves that hold a direct dependent of each MainSession step.
    let mut targets: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
    for (wave, ws) in &all {
        for dep in &ws.step.deps {
            if roles.get(dep.as_str()) == Some(&AgentRole::MainSession) {
                targets.entry(dep.as_str()).or_default().insert(*wave);
            }
        }
    }
    // A gate with no dependent stays in its own wave.
    for (wave, ws) in &all {
        if ws.role == AgentRole::MainSession {
            let set = targets.entry(ws.step.id.as_str()).or_default();
            if set.is_empty() {
                set.insert(*wave);
            }
        }
    }

    plan.waves
        .iter()
        .enumerate()
        .filter_map(|(i, wave)| {
            let gates: Vec<&WaveStep> = all
                .iter()
                .map(|(_, s)| *s)
                .filter(|s| {
                    s.role == AgentRole::MainSession
                        && targets
                            .get(s.step.id.as_str())
                            .is_some_and(|t| t.contains(&i))
                })
                .collect();
            let steps: Vec<&WaveStep> = wave
                .steps
                .iter()
                .filter(|s| s.role != AgentRole::MainSession)
                .collect();
            let ids: BTreeSet<&str> = gates
                .iter()
                .chain(steps.iter())
                .map(|s| s.step.id.as_str())
                .collect();
            let keep = |deps: &[String]| -> Vec<String> {
                deps.iter()
                    .filter(|d| ids.contains(d.as_str()))
                    .cloned()
                    .collect()
            };

            let mut out: Vec<GenStep> = gates
                .iter()
                .map(|g| GenStep {
                    id: g.step.id.clone(),
                    title: g.step.title.clone(),
                    kind: "gate",
                    agent: None,
                    deps: keep(&g.step.deps),
                    verify_cmd: Vec::new(),
                    prompt: Some(GATE_PROMPT.to_owned()),
                    gate: Some(
                        g.step
                            .gate
                            .clone()
                            .unwrap_or_else(|| DEFAULT_GATE.to_owned()),
                    ),
                })
                .collect();
            out.extend(steps.iter().map(|s| GenStep {
                id: s.step.id.clone(),
                title: s.step.title.clone(),
                kind: match s.role {
                    AgentRole::Builder => "impl",
                    AgentRole::Critic => "review",
                    AgentRole::Probe | AgentRole::MainSession => "test",
                },
                agent: Some(SHIM_AGENT),
                deps: keep(&s.step.deps),
                verify_cmd: shim_argv(source, &s.step.id, requested),
                prompt: Some(SHIM_PROMPT.to_owned()),
                gate: None,
            }));

            // WHERE the plan asks for merge on green, one merge step per Builder behind each
            // Critic (spec 0025 D15). The step runs the resolver under an outer `timeout`.
            if merge.merge_on_green {
                let wait = merge.merge_wait_secs.unwrap_or(DEFAULT_MERGE_WAIT_SECS) + 60;
                for critic in steps.iter().filter(|s| s.role == AgentRole::Critic) {
                    for builder in &critic.step.deps {
                        if roles.get(builder.as_str()) != Some(&AgentRole::Builder) {
                            continue;
                        }
                        out.push(GenStep {
                            id: format!("merge-{builder}"),
                            title: format!("Merge the PR of {builder}"),
                            kind: "test",
                            agent: Some(SHIM_AGENT),
                            deps: vec![critic.step.id.clone()],
                            verify_cmd: merge_argv(source, builder, wait, requested),
                            prompt: Some(SHIM_PROMPT.to_owned()),
                            gate: None,
                        });
                    }
                }
            }

            if out.is_empty() {
                return None;
            }
            Some(WaveFile {
                name: format!("{WAVE_PREFIX}{i}.toml"),
                flow: GeneratedFlow {
                    flow: FlowMetadata {
                        name: format!("{WAVE_PREFIX}{i}"),
                        description: None,
                    },
                    limits: Limits {
                        max_in_flight: plan.max_concurrency,
                    },
                    step: out,
                },
            })
        })
        .collect()
}

/// Quote `text` for a POSIX shell. A string of plain characters stays bare. Any other string is
/// wrapped in single quotes, and each single quote inside it becomes `'\\''`.
pub fn shell_quote(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:=+,@%-".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

/// Render one file: a header that names the command, then the TOML of the flow.
///
/// The header's command line parses with the CLI, so it names `source` and `out_dir` as given,
/// shell-quoted where a path holds other characters.
pub fn render(file: &WaveFile, source: &str, requested: usize, out_dir: &Path) -> Result<String> {
    let body = toml::to_string(&file.flow).context("serialize wave flow")?;
    let extra = if requested == 0 {
        String::new()
    } else {
        format!(" --max-concurrency {requested}")
    };
    Ok(format!(
        "# Generated by: cas wave flow --from {}{extra} --out-dir {}\n\
         # Do not edit by hand. Change the plan and run that command again.\n\n{body}",
        shell_quote(source),
        shell_quote(&out_dir.display().to_string())
    ))
}

/// Whether `name` is a generated wave file name.
fn is_wave_name(name: &str) -> bool {
    name.starts_with(WAVE_PREFIX) && name.ends_with(".toml")
}

/// `cas wave flow`: plan `from`, then write one file per wave into `out_dir`.
///
/// Stale wave files in `out_dir` are removed, so the directory holds exactly the plan's waves.
pub fn run_flow(from: &str, out_dir: &Path, max_concurrency: usize) -> Result<()> {
    let text = fs::read_to_string(from).with_context(|| format!("read plan {from}"))?;
    let def = SagaFlowDef::from_toml(&text).with_context(|| format!("parse plan {from}"))?;
    let merge = plan_merge(&text).with_context(|| format!("parse plan {from}"))?;
    let plan =
        WavePlan::plan(&def.to_steps(), max_concurrency).with_context(|| format!("plan {from}"))?;
    if merge.merge_on_green {
        // Refuse before any file is written or removed.
        let ids: BTreeSet<&str> = plan
            .waves
            .iter()
            .flat_map(|w| w.steps.iter().map(|s| s.step.id.as_str()))
            .collect();
        let builders = plan
            .waves
            .iter()
            .flat_map(|w| w.steps.iter())
            .filter(|s| s.role == AgentRole::Builder);
        for builder in builders {
            let clash = format!("merge-{}", builder.step.id);
            if ids.contains(clash.as_str()) {
                anyhow::bail!(
                    "plan {from} already holds the step id {clash}, which merge_on_green emits"
                );
            }
        }
    }
    let files = generate(&plan, from, max_concurrency, &merge);
    let rendered: Vec<(String, String)> = files
        .iter()
        .map(|f| render(f, from, max_concurrency, out_dir).map(|text| (f.name.clone(), text)))
        .collect::<Result<_>>()?;

    fs::create_dir_all(out_dir).with_context(|| format!("create {}", out_dir.display()))?;
    for entry in fs::read_dir(out_dir)? {
        let path = entry?.path();
        let stale = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| is_wave_name(n) && !rendered.iter().any(|(f, _)| f == n));
        if stale {
            fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        }
    }
    for (name, text) in &rendered {
        let path = out_dir.join(name);
        fs::write(&path, text).with_context(|| format!("write {}", path.display()))?;
        println!("{}", path.display());
    }
    Ok(())
}

/// The environment that `cas wave step` reads. Empty values count as unset.
#[derive(Debug, Clone, Default)]
pub struct WaveEnv {
    /// `CAS_WAVE_REPO`: the custody checkout.
    pub repo: Option<OsString>,
    /// `CAS_WAVE_STATE_DIR`: the state directory.
    pub state_dir: Option<OsString>,
    /// `CAS_WAVE_OPEN_PRS`: `1` turns on pull requests (spec 0025 D16).
    pub open_prs: Option<OsString>,
    /// `XDG_STATE_HOME`.
    pub xdg_state_home: Option<OsString>,
    /// `HOME`.
    pub home: Option<OsString>,
}

impl WaveEnv {
    /// Read the variables from the process environment.
    pub fn from_process() -> Self {
        Self {
            repo: std::env::var_os("CAS_WAVE_REPO"),
            state_dir: std::env::var_os("CAS_WAVE_STATE_DIR"),
            open_prs: std::env::var_os("CAS_WAVE_OPEN_PRS"),
            xdg_state_home: std::env::var_os("XDG_STATE_HOME"),
            home: std::env::var_os("HOME"),
        }
    }
}

/// The arguments of `cas wave step`.
#[derive(Debug, Clone)]
pub struct StepArgs {
    /// The plan that the step belongs to, as given to `--flow`.
    pub flow: String,
    /// The step id.
    pub step: String,
    /// The custody checkout, from `--repo`.
    pub repo: Option<PathBuf>,
    /// The state directory, from `--state-dir`.
    pub state_dir: Option<PathBuf>,
    /// The `--max-concurrency` of the plan, 0 for the default.
    pub max_concurrency: usize,
}

/// Return the value unless it is missing or empty.
fn set(value: &Option<OsString>) -> Option<&OsString> {
    value.as_ref().filter(|v| !v.is_empty())
}

/// `${XDG_STATE_HOME:-$HOME/.local/state}`.
fn state_root(env: &WaveEnv) -> PathBuf {
    if let Some(xdg) = set(&env.xdg_state_home) {
        return PathBuf::from(xdg);
    }
    let home = set(&env.home).map(PathBuf::from).unwrap_or_default();
    home.join(".local").join("state")
}

/// The state directory: `--state-dir`, then `CAS_WAVE_STATE_DIR`, then `cas/wave/<plan name>/`.
pub fn resolve_state_dir(flag: Option<&Path>, env: &WaveEnv, flow_name: &str) -> PathBuf {
    if let Some(dir) = flag {
        return dir.to_path_buf();
    }
    if let Some(dir) = set(&env.state_dir) {
        return PathBuf::from(dir);
    }
    state_root(env).join("cas").join("wave").join(flow_name)
}

/// The merge keys of a plan's `[flow]` table (spec 0025 D15). `FlowMetadata` stays untouched: it
/// is serialized into the saga flows, and these keys must not reach them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct PlanMerge {
    /// The plan asks for merge on green.
    #[serde(default)]
    pub merge_on_green: bool,
    /// How long a merge step waits for blackwall's merge-gate, in seconds.
    #[serde(default)]
    pub merge_wait_secs: Option<u64>,
}

/// Read the merge keys from the text of a plan. Other keys are ignored.
pub fn plan_merge(text: &str) -> Result<PlanMerge, toml::de::Error> {
    #[derive(Deserialize)]
    struct Top {
        #[serde(default)]
        flow: PlanMerge,
    }
    toml::from_str::<Top>(text).map(|top| top.flow)
}

/// `cas wave merge-gate` (spec 0025 D15). `args.step` is the Builder id. Exit 1: the executor
/// refuses (a Halted record, no PR, an unsigned commit) and no `blackwall` runs. Exit 2: the plan
/// cannot be read. Exit 3: the executor or the exec failed. Else the exit is blackwall's.
pub fn run_merge_gate(args: &StepArgs, env: &WaveEnv, cwd: &Path) -> i32 {
    match merge_gate_inner(args, env, cwd) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("cas wave merge-gate: {err:#}");
            3
        }
    }
}

fn merge_gate_inner(args: &StepArgs, env: &WaveEnv, cwd: &Path) -> Result<i32> {
    let text = match fs::read_to_string(&args.flow) {
        Ok(text) => text,
        Err(err) => return refuse(format!("read plan {}: {err}", args.flow)),
    };
    let (def, merge) = match (SagaFlowDef::from_toml(&text), plan_merge(&text)) {
        (Ok(def), Ok(merge)) => (def, merge),
        (Err(err), _) => return refuse(format!("parse plan {}: {err}", args.flow)),
        (_, Err(err)) => return refuse(format!("parse plan {}: {err}", args.flow)),
    };
    let plan = match WavePlan::plan(&def.to_steps(), args.max_concurrency) {
        Ok(plan) => plan,
        Err(err) => return refuse(format!("plan {}: {err}", args.flow)),
    };
    // No admission here: the resolver needs the record, not the plan's PR switch.
    let config = step_config(args, env, &def.flow.name, &merge, cwd);
    let repo = config.repo_dir.clone();
    let path_env = config.path_env.clone();
    let argv = match WaveExecutor::new(config, plan).merge_gate_argv(&args.step) {
        Ok(argv) => argv,
        Err(err @ WaveError::MergeGate(_)) => {
            eprintln!("{err}");
            return Ok(1);
        }
        Err(err) => return Err(err.into()),
    };
    // The environment twin beats `blackwall.toml` (F23): only the reviewed file may arm it.
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(&repo)
        .env_remove("BLACKWALL_AUTOMERGE");
    if let Some(path) = path_env {
        cmd.env("PATH", path);
    }
    let err = std::os::unix::process::CommandExt::exec(&mut cmd);
    eprintln!("cas wave merge-gate: exec {}: {err}", argv[0]);
    Ok(3)
}

/// The executor config of one `cas wave step` call.
///
/// The repo is `--repo`, then `CAS_WAVE_REPO`, then the working directory. The lock is host-wide,
/// so it lives under the state root and not under the state directory. Pull requests stay off
/// unless `CAS_WAVE_OPEN_PRS` is `1`.
pub fn step_config(
    args: &StepArgs,
    env: &WaveEnv,
    flow_name: &str,
    merge: &PlanMerge,
    cwd: &Path,
) -> WaveExecConfig {
    let repo = args
        .repo
        .clone()
        .or_else(|| set(&env.repo).map(PathBuf::from))
        .unwrap_or_else(|| cwd.to_path_buf());
    let state = resolve_state_dir(args.state_dir.as_deref(), env, flow_name);
    let lock = state_root(env).join("cas").join("build.lock");
    let mut config = WaveExecConfig::new(repo, state.clone(), state.join("scratch"), lock);
    config.open_prs = env.open_prs.as_ref().and_then(|v| v.to_str()) == Some("1");
    config.merge_on_green = merge.merge_on_green;
    if let Some(secs) = merge.merge_wait_secs {
        config.merge_wait = Duration::from_secs(secs);
    }
    config
}

/// Read every `*.json` record in `dir`. A missing directory holds no records.
fn read_records(dir: &Path) -> Result<Vec<Value>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err).with_context(|| format!("read {}", dir.display())),
    };
    let mut out = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            let value = serde_json::from_slice(&bytes)
                .with_context(|| format!("parse record {}", path.display()))?;
            out.push(value);
        }
    }
    Ok(out)
}

/// The `result` of a record's outcome: `passed`, `halted` or `awaiting_gate`.
fn outcome_result(record: &Value) -> Option<&str> {
    record.pointer("/outcome/result").and_then(Value::as_str)
}

/// Print one JSON line: the object with the repo, the working directory and any receipts.
fn emit(
    value: Value,
    repo: &Path,
    cwd: &Path,
    compensation: Option<&[CompensationReceipt]>,
) -> Result<()> {
    let mut map = match value {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("record".into(), other);
            map
        }
    };
    map.insert("repo".into(), Value::String(repo.display().to_string()));
    map.insert("cwd".into(), Value::String(cwd.display().to_string()));
    if let Some(receipts) = compensation {
        map.insert("compensation".into(), serde_json::to_value(receipts)?);
    }
    println!("{}", serde_json::to_string(&Value::Object(map))?);
    Ok(())
}

/// Exit codes of `cas wave step`: 0 passed, 1 halted, 2 refused or not ready (nothing written),
/// 3 the executor failed.
pub fn run_step(args: &StepArgs, env: &WaveEnv, cwd: &Path) -> i32 {
    match step_inner(args, env, cwd) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("cas wave step: {err:#}");
            3
        }
    }
}

fn refuse(message: impl std::fmt::Display) -> Result<i32> {
    eprintln!("cas wave step: {message}");
    Ok(2)
}

fn step_inner(args: &StepArgs, env: &WaveEnv, cwd: &Path) -> Result<i32> {
    let text = match fs::read_to_string(&args.flow) {
        Ok(text) => text,
        Err(err) => return refuse(format!("read plan {}: {err}", args.flow)),
    };
    let def = match SagaFlowDef::from_toml(&text) {
        Ok(def) => def,
        Err(err) => return refuse(format!("parse plan {}: {err}", args.flow)),
    };
    let merge = match plan_merge(&text) {
        Ok(merge) => merge,
        Err(err) => return refuse(format!("parse plan {}: {err}", args.flow)),
    };
    let plan = match WavePlan::plan(&def.to_steps(), args.max_concurrency) {
        Ok(plan) => plan,
        Err(err) => return refuse(format!("plan {}: {err}", args.flow)),
    };
    let Some(ws) = plan
        .waves
        .iter()
        .flat_map(|w| w.steps.iter())
        .find(|s| s.step.id == args.step)
    else {
        return refuse(format!("no step named {} in {}", args.step, args.flow));
    };
    let role = ws.role;

    let config = step_config(args, env, &def.flow.name, &merge, cwd);
    let repo = config.repo_dir.clone();
    let state = config.state_dir.clone();
    let records = read_records(&state)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // (1) A halt anywhere in the state directory stops every step.
    if let Some(halted) = records.iter().find(|r| outcome_result(r) == Some("halted")) {
        emit(halted.clone(), &repo, cwd, None)?;
        return Ok(1);
    }
    if role == AgentRole::MainSession {
        return refuse(format!(
            "step {} is a gate; gate steps belong to saga",
            args.step
        ));
    }

    // (2) A pass on the current wave base is final: print it and exit 0, before admission. A pass
    //     on another base, or with no wave base at all, is stale.
    let own = records
        .iter()
        .find(|r| {
            r.get("step").and_then(Value::as_str) == Some(args.step.as_str())
                && outcome_result(r) == Some("passed")
        })
        .cloned();
    if let Some(own) = own {
        let base = fs::read_to_string(repo.join(".blackwall").join("init-head"))
            .ok()
            .map(|t| t.trim().to_owned());
        let recorded = own.get("base").and_then(Value::as_str).map(str::to_owned);
        if base.is_some() && base == recorded {
            emit(own, &repo, cwd, None)?;
            return Ok(0);
        }
        return refuse(format!(
            "step {} passed on base {}, but the wave base is {}; the record is stale",
            args.step,
            recorded.as_deref().unwrap_or("(none)"),
            base.as_deref().unwrap_or("(no init-head)")
        ));
    }

    // (3) Everything else goes to the executor, which checks admission and readiness, runs the
    //     step and writes its record. An executor failure halts the step (spec 0025).
    let executor = WaveExecutor::new(config.clone(), plan);
    match executor.run_step(&args.step, now) {
        Ok(result) => match result.outcome {
            StepOutcome::Passed => match result.event {
                Some(event) => {
                    emit(serde_json::to_value(event)?, &repo, cwd, None)?;
                    Ok(0)
                }
                None => executor_failure(&config, &args.step, role, "a pass with no event", now),
            },
            StepOutcome::Halted(reason) => {
                let value = match result.event {
                    Some(event) => serde_json::to_value(event)?,
                    None => {
                        let mut map = Map::new();
                        map.insert("step".into(), Value::String(args.step.clone()));
                        map.insert("reason".into(), Value::String(format!("{reason:?}")));
                        Value::Object(map)
                    }
                };
                emit(value, &repo, cwd, Some(&result.compensation))?;
                Ok(1)
            }
            StepOutcome::NotReady(reason) => refuse(format!("not ready: {reason:?}")),
            StepOutcome::AwaitingGate => refuse(format!("step {} is a gate", args.step)),
        },
        Err(WaveError::Admit(err)) => refuse(format!("plan refused: {err}")),
        Err(err) => executor_failure(&config, &args.step, role, &format!("{err:#}"), now),
    }
}

/// An executor failure: write a `Halted` record with reason `ExecutorError` so that siblings and
/// later waves stop, print the reason, and exit 3.
fn executor_failure(
    config: &WaveExecConfig,
    step: &str,
    role: AgentRole,
    message: &str,
    now: u64,
) -> Result<i32> {
    eprintln!(
        "cas wave step: executor failed on step {step}: {message}; the step is halted with {}",
        HaltReason::ExecutorError.name()
    );
    if let Err(err) = record_halt(config, step, role, now) {
        eprintln!("cas wave step: could not write the halt record: {err:#}");
    }
    Ok(3)
}

/// Write the `Halted` record of `step` under the host lock, with the next sequence number.
fn record_halt(config: &WaveExecConfig, step: &str, role: AgentRole, now: u64) -> Result<()> {
    if let Some(dir) = config.lock_path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    fs::create_dir_all(&config.state_dir)
        .with_context(|| format!("create {}", config.state_dir.display()))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&config.lock_path)
        .with_context(|| format!("open {}", config.lock_path.display()))?;
    lock.lock()
        .with_context(|| format!("lock {}", config.lock_path.display()))?;
    let seq = read_records(&config.state_dir)?
        .iter()
        .filter_map(|r| r.get("seq").and_then(Value::as_u64))
        .max()
        .map_or(0, |m| m + 1);
    let mut outcome = Map::new();
    outcome.insert("result".into(), "halted".into());
    outcome.insert("reason".into(), HaltReason::ExecutorError.name().into());
    let mut record = Map::new();
    record.insert("step".into(), step.into());
    record.insert("role".into(), serde_json::to_value(role)?);
    record.insert("outcome".into(), Value::Object(outcome));
    record.insert("at".into(), now.into());
    record.insert("seq".into(), seq.into());
    let path = config.state_dir.join(format!("{step}.json"));
    let tmp = config.state_dir.join(format!("{step}.tmp"));
    fs::write(&tmp, serde_json::to_vec_pretty(&Value::Object(record))?)
        .with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}
