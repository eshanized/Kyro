pub mod auth;
pub mod catalog;
pub mod manager;
pub mod native;
pub mod remote_mcp;

pub use auth::{
    OAuthFlowInit, OAuthManager, OAuthSessionStatus, PendingOAuthSession, TokenExchangeResult,
};
pub use catalog::{get_default_catalog_items, ConnectorCatalogItem};
pub use manager::{
    canonicalize_json_value, classify_external_tool, compute_schema_fingerprint, AccountToolView,
    ConnectedAccountSummary, ConnectorKind, ConnectorManager, InvocationAuthProof,
    RegisteredConnectorTool,
};
pub use native::github::GitHubAdapter;
pub use native::NativeConnectorAdapter;
pub use remote_mcp::{RemoteMcpClient, ValidatedRemoteEndpoint};

use crate::execution::permissions::RiskLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub risk_level: RiskLevel,
    pub is_read_only: bool,
}
