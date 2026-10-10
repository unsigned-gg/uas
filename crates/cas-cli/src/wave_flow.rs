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
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use cas_engine::{
    AgentRole, CompensationReceipt, FlowMetadata, SagaFlowDef, StepOutcome, WaveError,
    WaveExecConfig, WaveExecutor, WavePlan, WaveStep, DEFAULT_MAX_WAVE_CONCURRENCY,
};
use serde::Serialize;
use serde_json::{Map, Value};

/// The file name prefix and flow name prefix of a generated wave.
const WAVE_PREFIX: &str = "cas-cpm-wave-w";
/// The prompt of every generated step: the step runs `verify_cmd` and nothing else (spec 0025 D2).
const SHIM_PROMPT: &str = "Do nothing. The verify_cmd runs this step. Report its exit.";
/// The approval gate of a MainSession step, unless the plan names another gate.
const DEFAULT_GATE: &str = "heimdal:approval";

/// The `[limits]` table of a generated flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Limits {
    /// The plan's `max_concurrency`.
    pub max_in_flight: usize,
}

/// One step of a generated flow. It never has an `agent` field: saga classifies it (spec F15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenStep {
    /// The step id, the same as the plan's.
    pub id: String,
    /// The step title, the same as the plan's.
    pub title: String,
    /// `impl` for a Builder, `review` for a Critic, `test` for a Probe, `gate` for a MainSession.
    pub kind: &'static str,
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

/// Build one flow per wave of `plan`. The output is deterministic: it depends only on the plan.
pub fn generate(plan: &WavePlan, source: &str) -> Vec<WaveFile> {
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
        .map(|(i, wave)| {
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
                    deps: keep(&g.step.deps),
                    verify_cmd: Vec::new(),
                    prompt: None,
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
                deps: keep(&s.step.deps),
                verify_cmd: vec![
                    "cas".into(),
                    "wave".into(),
                    "step".into(),
                    "--flow".into(),
                    source.into(),
                    "--step".into(),
                    s.step.id.clone(),
                ],
                prompt: Some(SHIM_PROMPT.to_owned()),
                gate: None,
            }));

            WaveFile {
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
            }
        })
        .collect()
}

/// Render one file: a header that names the command, then the TOML of the flow.
pub fn render(file: &WaveFile, source: &str) -> Result<String> {
    let body = toml::to_string(&file.flow).context("serialize wave flow")?;
    Ok(format!(
        "# Generated by: cas wave flow --from {source}\n\
         # Do not edit by hand. Change the plan and run that command again.\n\n{body}"
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
    let plan =
        WavePlan::plan(&def.to_steps(), max_concurrency).with_context(|| format!("plan {from}"))?;
    let files = generate(&plan, from);
    let rendered: Vec<(String, String)> = files
        .iter()
        .map(|f| render(f, from).map(|text| (f.name.clone(), text)))
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

/// The executor config of one `cas wave step` call.
///
/// The repo is `--repo`, then `CAS_WAVE_REPO`, then the working directory. The lock is host-wide,
/// so it lives under the state root and not under the state directory. Pull requests stay off
/// unless `CAS_WAVE_OPEN_PRS` is `1`.
pub fn step_config(args: &StepArgs, env: &WaveEnv, flow_name: &str, cwd: &Path) -> WaveExecConfig {
    let repo = args
        .repo
        .clone()
        .or_else(|| set(&env.repo).map(PathBuf::from))
        .unwrap_or_else(|| cwd.to_path_buf());
    let state = resolve_state_dir(args.state_dir.as_deref(), env, flow_name);
    let lock = state_root(env).join("cas").join("build.lock");
    let mut config = WaveExecConfig::new(repo, state.clone(), state.join("scratch"), lock);
    config.open_prs = env.open_prs.as_ref().and_then(|v| v.to_str()) == Some("1");
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
    // The step always plans with the default concurrency, as `cas wave flow` does for the
    // checked-in plan. The verify_cmd carries no flag, so a plan run with another limit needs the
    // same limit here.
    let plan = match WavePlan::plan(&def.to_steps(), DEFAULT_MAX_WAVE_CONCURRENCY) {
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

    let config = step_config(args, env, &def.flow.name, cwd);
    let repo = config.repo_dir.clone();
    let state = config.state_dir.clone();
    let records = read_records(&state)?;

    // (1) A halt anywhere in the state directory stops every step.
    if let Some(halted) = records.iter().find(|r| outcome_result(r) == Some("halted")) {
        emit(halted.clone(), &repo, cwd, None)?;
        return Ok(1);
    }
    if ws.role == AgentRole::MainSession {
        return refuse(format!(
            "step {} is a gate; gate steps belong to saga",
            args.step
        ));
    }

    // (2) This step's own pass is final for its wave base. Another base is stale.
    let own = records
        .iter()
        .find(|r| {
            r.get("step").and_then(Value::as_str) == Some(args.step.as_str())
                && outcome_result(r) == Some("passed")
        })
        .cloned();
    if let Some(own) = &own {
        let base = fs::read_to_string(repo.join(".blackwall").join("init-head"))
            .ok()
            .map(|t| t.trim().to_owned());
        let recorded = own.get("base").and_then(Value::as_str);
        if base.as_deref() != recorded {
            return refuse(format!(
                "step {} passed on base {}, but the wave base is {}; the record is stale",
                args.step,
                recorded.unwrap_or("(none)"),
                base.as_deref().unwrap_or("(no init-head)")
            ));
        }
    }

    // (3) Everything else goes to the executor, which checks readiness and writes the record.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let executor = WaveExecutor::new(config, plan);
    match executor.run_step(&args.step, now) {
        Ok(result) => match result.outcome {
            StepOutcome::Passed => {
                let value = match (result.event, own) {
                    (Some(event), _) => serde_json::to_value(event)?,
                    (None, Some(own)) => own,
                    (None, None) => return Ok(3),
                };
                emit(value, &repo, cwd, None)?;
                Ok(0)
            }
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
        Err(err) => Err(err.into()),
    }
}
