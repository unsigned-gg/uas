//! Pure DDD domain representations for zero-copy IPC and shared memory payloads (Spec 0029).
//! Zero I/O, zero suite dependencies.

use serde::{Deserialize, Serialize};

/// Delivery tier for an IPC payload based on payload size and channel capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcDeliveryTier {
    /// In-band prompt injection (< 8 KB). No secondary channel lookup required.
    InBand,
    /// Direct memory access via sealed memfd/POSIX shm and SCM_RIGHTS (>= 8 KB).
    SharedMemoryDma,
    /// Direct server push (e.g. local loopback HTTP/socket push).
    DirectPush,
}

impl IpcDeliveryTier {
    /// Threshold (8 KB) separating direct in-band prompt injection from zero-copy DMA.
    pub const IN_BAND_MAX_BYTES: usize = 8192;

    /// Resolve the appropriate delivery tier for a given payload size in bytes.
    pub fn for_size(size_bytes: usize) -> Self {
        if size_bytes < Self::IN_BAND_MAX_BYTES {
            Self::InBand
        } else {
            Self::SharedMemoryDma
        }
    }
}

/// Metadata descriptor for an IPC payload transferred via shared memory or in-band.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcPayloadDescriptor {
    /// Unique identifier for this payload (e.g. message UUID or run/step id).
    pub id: String,
    /// Classification of payload content (e.g. "prompt", "signal", "artifact", "diff").
    pub kind: String,
    /// Size of the payload buffer in bytes.
    pub size_bytes: u64,
    /// Whether the memory buffer is sealed with kernel-enforced immutability.
    pub sealed: bool,
    /// Unix timestamp in milliseconds when this payload descriptor was produced.
    pub created_at_ms: u64,
}

impl IpcPayloadDescriptor {
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        size_bytes: u64,
        sealed: bool,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            size_bytes,
            sealed,
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    /// Returns the recommended delivery tier for this payload.
    pub fn delivery_tier(&self) -> IpcDeliveryTier {
        IpcDeliveryTier::for_size(self.size_bytes as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delivery_tier_thresholds() {
        assert_eq!(IpcDeliveryTier::for_size(0), IpcDeliveryTier::InBand);
        assert_eq!(IpcDeliveryTier::for_size(8191), IpcDeliveryTier::InBand);
        assert_eq!(
            IpcDeliveryTier::for_size(8192),
            IpcDeliveryTier::SharedMemoryDma
        );
        assert_eq!(
            IpcDeliveryTier::for_size(100_000),
            IpcDeliveryTier::SharedMemoryDma
        );
    }

    #[test]
    fn test_descriptor_serde_roundtrip() {
        let desc = IpcPayloadDescriptor::new("msg-1234", "prompt", 16384, true);
        assert_eq!(desc.delivery_tier(), IpcDeliveryTier::SharedMemoryDma);
        let serialized = serde_json::to_string(&desc).expect("serialize");
        let deserialized: IpcPayloadDescriptor =
            serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(desc, deserialized);
    }
}
