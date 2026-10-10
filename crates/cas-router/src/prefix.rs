//! Canonical prompt prefix serialization and hashing (spec 0017 D1, D2).
//!
//! The canonical form is `SYS:<system parts joined by \n>|TOOLS:<name:json joined by |>`,
//! with tools sorted by name, then by json bytes. The hash is the first 16 bytes of its
//! SHA-256. The hash is stable across Rust releases, so sticky routing holds across hosts.

use sha2::{Digest, Sha256};

/// One tool definition: a name and its JSON schema text, hashed as given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDef {
    /// Tool name, the first sort key.
    pub name: String,
    /// The tool JSON as the client sent it, the second sort key.
    pub json: String,
}

/// The invariant part of a request: system text and tool definitions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrefixInput {
    /// System instruction parts, in request order.
    pub system: Vec<String>,
    /// Tool definitions, in any order.
    pub tools: Vec<ToolDef>,
}

/// The first 16 bytes of the SHA-256 of the canonical prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrefixHash(pub [u8; 16]);

impl PrefixInput {
    /// Hash the canonical form of this prefix.
    pub fn hash(&self) -> PrefixHash {
        let digest = Sha256::digest(self.canonical().as_bytes());
        let mut first = [0u8; 16];
        first.copy_from_slice(&digest[..16]);
        PrefixHash(first)
    }

    /// The canonical UTF-8 string that is hashed.
    fn canonical(&self) -> String {
        let mut tools: Vec<&ToolDef> = self.tools.iter().collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.json.cmp(&b.json)));
        let system = self.system.join("\n");
        let tools = tools
            .iter()
            .map(|t| format!("{}:{}", t.name, t.json))
            .collect::<Vec<_>>()
            .join("|");
        format!("SYS:{system}|TOOLS:{tools}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(h: &PrefixHash) -> String {
        h.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn tool(name: &str, json: &str) -> ToolDef {
        ToolDef {
            name: name.to_string(),
            json: json.to_string(),
        }
    }

    #[test]
    fn prefix_hash_known_vector() {
        let input = PrefixInput {
            system: vec!["You are a builder.".to_string()],
            tools: vec![
                tool("write", r#"{"type":"function"}"#),
                tool("read", r#"{"type":"function"}"#),
            ],
        };
        assert_eq!(hex(&input.hash()), "0cd0bd32e200ee0cb8655991dd25da9d");
    }

    #[test]
    fn canonical_form_sorts_tools_and_handles_zero_tools() {
        let with_tools = PrefixInput {
            system: vec!["a".to_string(), "b".to_string()],
            tools: vec![
                tool("write", "{}"),
                tool("read", "{}"),
                tool("read", "{\"x\":1}"),
            ],
        };
        assert_eq!(
            with_tools.canonical(),
            "SYS:a\nb|TOOLS:read:{\"x\":1}|read:{}|write:{}"
        );
        let none = PrefixInput {
            system: vec!["s".to_string()],
            tools: vec![],
        };
        assert_eq!(none.canonical(), "SYS:s|TOOLS:");
    }

    #[test]
    fn prefix_hash_ignores_tool_order() {
        let tools = [
            tool("a", "{}"),
            tool("b", "{\"k\":2}"),
            tool("c", "{\"k\":3}"),
        ];
        let orders: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let first = PrefixInput {
            system: vec!["s".to_string()],
            tools: tools.to_vec(),
        }
        .hash();
        for order in orders {
            let permuted = PrefixInput {
                system: vec!["s".to_string()],
                tools: order.iter().map(|&i| tools[i].clone()).collect(),
            };
            assert_eq!(permuted.hash(), first, "order {order:?}");
        }
    }

    #[test]
    fn prefix_hash_changes_with_system() {
        let a = PrefixInput {
            system: vec!["one".to_string()],
            tools: vec![],
        };
        let b = PrefixInput {
            system: vec!["two".to_string()],
            tools: vec![],
        };
        assert_ne!(a.hash(), b.hash());
    }
}
