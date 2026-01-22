use std::sync::Arc;

use rmcp::{
  handler::server::{tool::ToolRouter, wrapper::Parameters},
  model::*,
  schemars, tool, tool_router, ErrorData as McpError, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;
use uuid::Uuid;

use crate::db::DatabaseBackend;
use crate::query::QueryEnginePool;

// Parameter structs for tool inputs
#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryParams {
  /// JavaScript query (e.g., db.table("users").filter(u => u.age > 21).run())
  pub query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InsertParams {
  /// Collection name
  pub collection: String,
  /// Document data to insert
  pub data: serde_json::Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateParams {
  /// Collection name
  pub collection: String,
  /// Document UUID
  pub id: String,
  /// New document data
  pub data: serde_json::Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteParams {
  /// Collection name
  pub collection: String,
  /// Document UUID
  pub id: String,
}

#[derive(Clone)]
pub struct McpServer {
  backend: Arc<dyn DatabaseBackend>,
  engine_pool: Arc<QueryEnginePool>,
  #[allow(dead_code)] // Used by #[tool_router] macro
  tool_router: ToolRouter<Self>,
}

#[tool_router]
impl McpServer {
  pub fn new(backend: Arc<dyn DatabaseBackend>, engine_pool: Arc<QueryEnginePool>) -> Self {
    Self {
      backend,
      engine_pool,
      tool_router: Self::tool_router(),
    }
  }

  #[tool(description = "Execute a SquirrelDB JavaScript query")]
  async fn query(&self, params: Parameters<QueryParams>) -> Result<CallToolResult, McpError> {
    let result = self
      .engine_pool
      .execute(&params.0.query, self.backend.as_ref())
      .await
      .map_err(|e| McpError::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(
      serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
    )]))
  }

  #[tool(description = "Insert a document into a collection")]
  async fn insert(&self, params: Parameters<InsertParams>) -> Result<CallToolResult, McpError> {
    let doc = self
      .backend
      .insert(&params.0.collection, params.0.data.clone())
      .await
      .map_err(|e| McpError::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(
      serde_json::to_string_pretty(&doc).unwrap_or_default(),
    )]))
  }

  #[tool(description = "Update a document by ID")]
  async fn update(&self, params: Parameters<UpdateParams>) -> Result<CallToolResult, McpError> {
    let uuid =
      Uuid::parse_str(&params.0.id).map_err(|e| McpError::invalid_params(e.to_string(), None))?;

    let doc = self
      .backend
      .update(&params.0.collection, uuid, params.0.data.clone())
      .await
      .map_err(|e| McpError::internal_error(e.to_string(), None))?;

    match doc {
      Some(d) => Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&d).unwrap_or_default(),
      )])),
      None => Ok(CallToolResult::success(vec![Content::text(
        r#"{"error": "Document not found"}"#,
      )])),
    }
  }

  #[tool(description = "Delete a document by ID")]
  async fn delete(&self, params: Parameters<DeleteParams>) -> Result<CallToolResult, McpError> {
    let uuid =
      Uuid::parse_str(&params.0.id).map_err(|e| McpError::invalid_params(e.to_string(), None))?;

    let doc = self
      .backend
      .delete(&params.0.collection, uuid)
      .await
      .map_err(|e| McpError::internal_error(e.to_string(), None))?;

    match doc {
      Some(d) => Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&d).unwrap_or_default(),
      )])),
      None => Ok(CallToolResult::success(vec![Content::text(
        r#"{"error": "Document not found"}"#,
      )])),
    }
  }

  #[tool(description = "List all collections in the database")]
  async fn list_collections(&self) -> Result<CallToolResult, McpError> {
    let collections = self
      .backend
      .list_collections()
      .await
      .map_err(|e| McpError::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(
      serde_json::to_string_pretty(&collections).unwrap_or_default(),
    )]))
  }
}

impl ServerHandler for McpServer {
  fn get_info(&self) -> ServerInfo {
    ServerInfo {
      protocol_version: ProtocolVersion::LATEST,
      capabilities: ServerCapabilities::builder()
        .enable_tools()
        .build(),
      server_info: Implementation {
        name: "squirreldb".into(),
        title: Some("SquirrelDB".into()),
        version: env!("CARGO_PKG_VERSION").into(),
        icons: None,
        website_url: None,
      },
      instructions: Some("SquirrelDB MCP server. Use query tool for JavaScript queries, or insert/update/delete for direct CRUD operations.".into()),
    }
  }
}

impl McpServer {
  /// Run MCP server over stdio transport
  pub async fn run_stdio(
    backend: Arc<dyn DatabaseBackend>,
    engine_pool: Arc<QueryEnginePool>,
  ) -> Result<(), anyhow::Error> {
    let server = Self::new(backend, engine_pool);
    let transport = rmcp::transport::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
  }

  /// Run MCP server over SSE transport
  pub async fn run_sse(
    addr: &str,
    backend: Arc<dyn DatabaseBackend>,
    engine_pool: Arc<QueryEnginePool>,
  ) -> Result<(), anyhow::Error> {
    use axum::Router;
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
    use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
    use std::net::SocketAddr;

    let addr: SocketAddr = addr.parse()?;

    let backend = backend.clone();
    let engine_pool = engine_pool.clone();

    let config = StreamableHttpServerConfig::default();
    let session_manager = Arc::new(LocalSessionManager::default());

    let service = StreamableHttpService::new(
      move || Ok(McpServer::new(backend.clone(), engine_pool.clone())),
      session_manager,
      config,
    );

    let app = Router::new().route("/mcp", axum::routing::any_service(service));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
  }
}
