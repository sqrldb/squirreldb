use std::sync::Arc;
use std::time::Duration;

use rmcp::{
  handler::server::{tool::ToolRouter, wrapper::Parameters},
  model::*,
  schemars, tool, tool_router, ErrorData as McpError, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;
use uuid::Uuid;

use crate::cache::{CacheStore, CacheValue, InMemoryCacheStore};
use crate::db::DatabaseBackend;
use crate::query::QueryEnginePool;
use crate::types::DEFAULT_PROJECT_ID;

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

// Cache parameter structs
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CacheGetParams {
  /// Cache key
  pub key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CacheSetParams {
  /// Cache key
  pub key: String,
  /// Value to cache (string or JSON)
  pub value: String,
  /// TTL in seconds (optional, 0 = no expiry)
  #[serde(default)]
  pub ttl: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CacheDelParams {
  /// Cache key to delete
  pub key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CacheKeysParams {
  /// Pattern to match keys (e.g., "user:*")
  #[serde(default = "default_pattern")]
  pub pattern: String,
}

fn default_pattern() -> String {
  "*".to_string()
}

#[derive(Clone)]
pub struct McpServer {
  backend: Arc<dyn DatabaseBackend>,
  engine_pool: Arc<QueryEnginePool>,
  cache_store: Option<Arc<InMemoryCacheStore>>,
  #[allow(dead_code)] // Used by #[tool_router] macro
  tool_router: ToolRouter<Self>,
}

#[tool_router]
impl McpServer {
  pub fn new(backend: Arc<dyn DatabaseBackend>, engine_pool: Arc<QueryEnginePool>) -> Self {
    Self {
      backend,
      engine_pool,
      cache_store: None,
      tool_router: Self::tool_router(),
    }
  }

  pub fn with_cache(
    backend: Arc<dyn DatabaseBackend>,
    engine_pool: Arc<QueryEnginePool>,
    cache_store: Arc<InMemoryCacheStore>,
  ) -> Self {
    Self {
      backend,
      engine_pool,
      cache_store: Some(cache_store),
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
      .insert(
        DEFAULT_PROJECT_ID,
        &params.0.collection,
        params.0.data.clone(),
      )
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
      .update(
        DEFAULT_PROJECT_ID,
        &params.0.collection,
        uuid,
        params.0.data.clone(),
      )
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
      .delete(DEFAULT_PROJECT_ID, &params.0.collection, uuid)
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
      .list_collections(DEFAULT_PROJECT_ID)
      .await
      .map_err(|e| McpError::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text(
      serde_json::to_string_pretty(&collections).unwrap_or_default(),
    )]))
  }

  // Cache tools

  #[tool(description = "Get a value from the cache by key")]
  async fn cache_get(
    &self,
    params: Parameters<CacheGetParams>,
  ) -> Result<CallToolResult, McpError> {
    let store = self
      .cache_store
      .as_ref()
      .ok_or_else(|| McpError::internal_error("Cache not enabled", None))?;

    match store.get(&params.0.key).await {
      Some(entry) => Ok(CallToolResult::success(vec![Content::text(
        entry.value.to_resp_string(),
      )])),
      None => Ok(CallToolResult::success(vec![Content::text("(nil)")])),
    }
  }

  #[tool(description = "Set a value in the cache with optional TTL")]
  async fn cache_set(
    &self,
    params: Parameters<CacheSetParams>,
  ) -> Result<CallToolResult, McpError> {
    let store = self
      .cache_store
      .as_ref()
      .ok_or_else(|| McpError::internal_error("Cache not enabled", None))?;

    let ttl = if params.0.ttl > 0 {
      Some(Duration::from_secs(params.0.ttl))
    } else {
      None
    };

    let value: CacheValue = params.0.value.as_str().into();

    store
      .set(&params.0.key, value, ttl)
      .await
      .map_err(|e| McpError::internal_error(e.to_string(), None))?;

    Ok(CallToolResult::success(vec![Content::text("OK")]))
  }

  #[tool(description = "Delete a key from the cache")]
  async fn cache_del(
    &self,
    params: Parameters<CacheDelParams>,
  ) -> Result<CallToolResult, McpError> {
    let store = self
      .cache_store
      .as_ref()
      .ok_or_else(|| McpError::internal_error("Cache not enabled", None))?;

    let deleted = store.delete(&params.0.key).await;
    Ok(CallToolResult::success(vec![Content::text(if deleted {
      "1"
    } else {
      "0"
    })]))
  }

  #[tool(description = "List cache keys matching a pattern")]
  async fn cache_keys(
    &self,
    params: Parameters<CacheKeysParams>,
  ) -> Result<CallToolResult, McpError> {
    let store = self
      .cache_store
      .as_ref()
      .ok_or_else(|| McpError::internal_error("Cache not enabled", None))?;

    let keys = store.keys(&params.0.pattern).await;
    Ok(CallToolResult::success(vec![Content::text(
      serde_json::to_string_pretty(&keys).unwrap_or_default(),
    )]))
  }

  #[tool(description = "Get cache statistics and info")]
  async fn cache_info(&self) -> Result<CallToolResult, McpError> {
    let store = self
      .cache_store
      .as_ref()
      .ok_or_else(|| McpError::internal_error("Cache not enabled", None))?;

    let stats = store.info().await;
    let info = serde_json::json!({
      "keys": stats.keys,
      "memory_used": stats.memory_used,
      "memory_limit": stats.memory_limit,
      "hits": stats.hits,
      "misses": stats.misses,
      "evictions": stats.evictions,
      "hit_rate": if stats.hits + stats.misses > 0 {
        (stats.hits as f64) / ((stats.hits + stats.misses) as f64) * 100.0
      } else {
        0.0
      }
    });

    Ok(CallToolResult::success(vec![Content::text(
      serde_json::to_string_pretty(&info).unwrap_or_default(),
    )]))
  }

  #[tool(description = "Flush all keys from the cache")]
  async fn cache_flush(&self) -> Result<CallToolResult, McpError> {
    let store = self
      .cache_store
      .as_ref()
      .ok_or_else(|| McpError::internal_error("Cache not enabled", None))?;

    store.flush().await;
    Ok(CallToolResult::success(vec![Content::text("OK")]))
  }
}

impl ServerHandler for McpServer {
  fn get_info(&self) -> ServerInfo {
    let cache_note = if self.cache_store.is_some() {
      " Cache tools (cache_get, cache_set, cache_del, cache_keys, cache_info, cache_flush) available."
    } else {
      ""
    };

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
      instructions: Some(format!(
        "SquirrelDB MCP server. Use query tool for JavaScript queries, or insert/update/delete for direct CRUD operations.{}",
        cache_note
      )),
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

  /// Run MCP server over stdio transport with cache support
  pub async fn run_stdio_with_cache(
    backend: Arc<dyn DatabaseBackend>,
    engine_pool: Arc<QueryEnginePool>,
    cache_store: Arc<InMemoryCacheStore>,
  ) -> Result<(), anyhow::Error> {
    let server = Self::with_cache(backend, engine_pool, cache_store);
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

  /// Run MCP server over SSE transport with cache support
  pub async fn run_sse_with_cache(
    addr: &str,
    backend: Arc<dyn DatabaseBackend>,
    engine_pool: Arc<QueryEnginePool>,
    cache_store: Arc<InMemoryCacheStore>,
  ) -> Result<(), anyhow::Error> {
    use axum::Router;
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
    use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
    use std::net::SocketAddr;

    let addr: SocketAddr = addr.parse()?;

    let backend = backend.clone();
    let engine_pool = engine_pool.clone();
    let cache_store = cache_store.clone();

    let config = StreamableHttpServerConfig::default();
    let session_manager = Arc::new(LocalSessionManager::default());

    let service = StreamableHttpService::new(
      move || {
        Ok(McpServer::with_cache(
          backend.clone(),
          engine_pool.clone(),
          cache_store.clone(),
        ))
      },
      session_manager,
      config,
    );

    let app = Router::new().route("/mcp", axum::routing::any_service(service));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
  }
}
