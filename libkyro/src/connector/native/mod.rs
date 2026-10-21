pub mod github;
pub mod google;

use crate::connector::ConnectorToolDefinition;
use crate::storage::{ConnectedAccountRecord, Vault};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait NativeConnectorAdapter: Send + Sync {
    fn provider_id(&self) -> &str;
    fn list_tools(&self, account: &ConnectedAccountRecord) -> Vec<ConnectorToolDefinition>;
    async fn execute_tool(
        &self,
        account: &ConnectedAccountRecord,
        tool_name: &str,
        arguments: Value,
        vault: &Vault,
    ) -> Result<String, String>;
    async fn test_connection(
        &self,
        account: &ConnectedAccountRecord,
        vault: &Vault,
    ) -> Result<String, String>;
}
