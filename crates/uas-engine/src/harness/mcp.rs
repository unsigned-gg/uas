// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Agent harness Model Context Protocol (MCP) configuration bridge (spec 0063).
//!
//! Synthesizes slot-isolated `mcp-config.json` files, enforces role-based tool capabilities,
//! masks untrusted repository configurations, and bridges isolated Unix domain sockets
//! to standard input/output MCP transports.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uas_domain::SlotId;

use crate::harness::is_secret_env_key;
use crate::wave::AgentRole;
use crate::EngineError;

/// Default approved MCP server executable commands.
pub const DEFAULT_APPROVED_MCP_COMMANDS: &[&str] =
    &["uas-bridge", "node", "python", "python3", "npx", "uvx"];

/// Transport mechanism used by an MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransport {
    /// Standard input and output process transport.
    Stdio {
        /// Executable command or path.
        command: String,
        /// Command line arguments passed to the executable.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },
    /// Unix domain socket proxy transport bridged through `uas-bridge`.
    UnixSocket {
        /// Path to the Unix domain socket endpoint.
        path: PathBuf,
    },
}

impl McpTransport {
    /// Creates a standard input and output transport.
    pub fn stdio(
        command: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Stdio {
            command: command.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    /// Creates a Unix domain socket proxy transport.
    pub fn unix_socket(path: impl Into<PathBuf>) -> Self {
        Self::UnixSocket { path: path.into() }
    }

    /// Resolves the executable command and arguments for this transport.
    #[must_use]
    pub fn to_command_and_args(&self) -> (String, Vec<String>) {
        match self {
            Self::Stdio { command, args } => (command.clone(), args.clone()),
            Self::UnixSocket { path } => (
                "uas-bridge".to_string(),
                vec!["--socket".to_string(), path.to_string_lossy().to_string()],
            ),
        }
    }
}

/// Configuration definition for an individual Model Context Protocol server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Transport used to communicate with the server.
    pub transport: McpTransport,
    /// Environment variables injected into the server process.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Whether this server provides strictly read-only capabilities.
    #[serde(default)]
    pub read_only: bool,
}

impl McpServerConfig {
    /// Creates a new MCP server configuration.
    #[must_use]
    pub fn new(transport: McpTransport) -> Self {
        Self {
            transport,
            env: BTreeMap::new(),
            read_only: false,
        }
    }

    /// Creates a standard input and output MCP server configuration.
    pub fn stdio(
        command: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::new(McpTransport::stdio(command, args))
    }

    /// Creates a Unix domain socket MCP server configuration.
    pub fn unix_socket(path: impl Into<PathBuf>) -> Self {
        Self::new(McpTransport::unix_socket(path))
    }

    /// Creates a Reverie memory access server bridging a Unix domain socket path.
    pub fn reverie_uds(path: impl Into<PathBuf>, read_only: bool) -> Self {
        Self::unix_socket(path).with_read_only(read_only)
    }

    /// Creates a Reverie memory access server for a specific slot proxy path.
    pub fn reverie_proxy(slot_id: &SlotId, read_only: bool) -> Self {
        let path = PathBuf::from(format!("/tmp/uas-reverie-proxy/{}.sock", slot_id.as_str()));
        Self::reverie_uds(path, read_only)
    }

    /// Sets whether the server is read-only.
    #[must_use]
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Adds an environment variable, scrubbing secrets.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let k = key.into();
        let v = value.into();
        if !is_secret_env_key(&k) {
            self.env.insert(k, v);
        }
        self
    }

    /// Adds multiple environment variables, scrubbing any secret keys.
    #[must_use]
    pub fn with_env_map(
        mut self,
        map: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        for (k, v) in map {
            let key = k.into();
            let val = v.into();
            if !is_secret_env_key(&key) {
                self.env.insert(key, val);
            }
        }
        self
    }

    /// Returns a copy of the environment map with all secret-named keys removed.
    #[must_use]
    pub fn scrubbed_env(&self) -> BTreeMap<String, String> {
        self.env
            .iter()
            .filter(|(k, _)| !is_secret_env_key(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// JSON representation of an MCP server inside `mcp-config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerJson {
    /// Executable command.
    pub command: String,
    /// Command line arguments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Environment variables.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// Root JSON configuration structure for standard Model Context Protocol clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpRootConfig {
    /// Dictionary of MCP server definitions.
    #[serde(rename = "mcpServers")]
    pub mcp_servers: BTreeMap<String, McpServerJson>,
}

impl McpRootConfig {
    /// Serializes this configuration to formatted JSON text.
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Converts this configuration into a `serde_json::Value`.
    pub fn to_json_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// Writes the configuration to a specified file path.
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = self
            .to_json_string()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(p, json)?;
        Ok(())
    }

    /// Writes the configuration to `mcp-config.json` inside the specified scratch directory.
    pub fn write_to_scratch(
        &self,
        scratch_dir: impl AsRef<Path>,
    ) -> Result<PathBuf, std::io::Error> {
        let target = scratch_dir.as_ref().join("mcp-config.json");
        self.write_to_file(&target)?;
        Ok(target)
    }
}

/// Builder for synthesizing isolated, role-specific Model Context Protocol configurations.
#[derive(Debug, Clone)]
pub struct McpConfigBuilder {
    role: AgentRole,
    servers: BTreeMap<String, McpServerConfig>,
    allowed_commands: BTreeSet<String>,
}

impl Default for McpConfigBuilder {
    fn default() -> Self {
        Self::new(AgentRole::Builder)
    }
}

impl McpConfigBuilder {
    /// Creates a new builder for the specified agent role.
    #[must_use]
    pub fn new(role: AgentRole) -> Self {
        let mut allowed = BTreeSet::new();
        for cmd in DEFAULT_APPROVED_MCP_COMMANDS {
            allowed.insert((*cmd).to_string());
        }
        Self {
            role,
            servers: BTreeMap::new(),
            allowed_commands: allowed,
        }
    }

    /// Creates a builder for a Builder agent role.
    #[must_use]
    pub fn builder() -> Self {
        Self::new(AgentRole::Builder)
    }

    /// Creates a builder for a Critic agent role.
    #[must_use]
    pub fn critic() -> Self {
        Self::new(AgentRole::Critic)
    }

    /// Sets the target agent role.
    pub fn with_role(&mut self, role: AgentRole) -> &mut Self {
        self.role = role;
        self
    }

    /// Adds an approved executable command to the allowlist.
    pub fn allow_command(&mut self, command: impl Into<String>) -> &mut Self {
        self.allowed_commands.insert(command.into());
        self
    }

    /// Replaces or extends the allowed commands list.
    pub fn with_allowed_commands(
        &mut self,
        commands: impl IntoIterator<Item = impl Into<String>>,
    ) -> &mut Self {
        for cmd in commands {
            self.allowed_commands.insert(cmd.into());
        }
        self
    }

    /// Registers a server configuration with a unique name.
    pub fn add_server(&mut self, name: impl Into<String>, config: McpServerConfig) -> &mut Self {
        self.servers.insert(name.into(), config);
        self
    }

    /// Registers a standard input/output server configuration.
    pub fn add_stdio_server(
        &mut self,
        name: impl Into<String>,
        command: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
        read_only: bool,
    ) -> &mut Self {
        let config = McpServerConfig::stdio(command, args).with_read_only(read_only);
        self.add_server(name, config)
    }

    /// Registers a Reverie memory Unix domain socket bridge.
    pub fn add_reverie_uds(
        &mut self,
        socket_path: impl Into<PathBuf>,
        read_only: bool,
    ) -> &mut Self {
        let config = McpServerConfig::reverie_uds(socket_path, read_only);
        self.add_server("reverie", config)
    }

    /// Registers a Reverie proxy endpoint for a specific slot.
    pub fn add_reverie_proxy(&mut self, slot_id: &SlotId, read_only: bool) -> &mut Self {
        let config = McpServerConfig::reverie_proxy(slot_id, read_only);
        self.add_server("reverie", config)
    }

    /// Checks if a command is permitted by the allowlist.
    fn is_command_allowed(&self, command: &str) -> bool {
        if self.allowed_commands.contains(command) {
            return true;
        }
        let base_name = Path::new(command)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(command);
        self.allowed_commands.contains(base_name)
    }

    /// Validates and synthesizes the final root MCP configuration.
    pub fn build(&self) -> Result<McpRootConfig, EngineError> {
        let mut mcp_servers = BTreeMap::new();

        for (name, config) in &self.servers {
            let (command, args) = config.transport.to_command_and_args();

            // Fail-closed authorization check on server command (REQ-0063-07).
            if !self.is_command_allowed(&command) {
                return Err(EngineError::McpServerUnauthorized {
                    server: name.clone(),
                    command,
                });
            }

            // Role capability filtering (REQ-0063-06).
            // Critic and Probe roles strictly require read_only = true.
            if (self.role == AgentRole::Critic || self.role == AgentRole::Probe)
                && !config.read_only
            {
                continue;
            }

            // Scrub secret-named environment variables (REQ-0063-08).
            let env = config.scrubbed_env();

            mcp_servers.insert(name.clone(), McpServerJson { command, args, env });
        }

        Ok(McpRootConfig { mcp_servers })
    }

    /// Synthesizes the configuration (alias for `build`).
    pub fn synthesize(&self) -> Result<McpRootConfig, EngineError> {
        self.build()
    }

    /// Synthesizes the configuration and writes it to `<scratch_dir>/mcp-config.json`.
    pub fn write_to_scratch(&self, scratch_dir: impl AsRef<Path>) -> Result<PathBuf, EngineError> {
        let config = self.build()?;
        config
            .write_to_scratch(scratch_dir)
            .map_err(|e| EngineError::StoreError(format!("failed to write mcp-config.json: {e}")))
    }
}
