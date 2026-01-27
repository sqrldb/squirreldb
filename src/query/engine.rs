use std::sync::atomic::{AtomicUsize, Ordering};

use lru::LruCache;
use parking_lot::Mutex;

use super::QueryCompiler;
use crate::db::{DatabaseBackend, SqlDialect};
use crate::types::{
  ChangesOptions, CompiledFilter, Document, FilterSpec, OrderBySpec, OrderDirection, QuerySpec,
};
use rquickjs::{Context, Function, Runtime, Value};

/// Pool of QueryEngine instances for sharing across connections.
/// This reduces memory from 10MB × connections to 10MB × pool_size.
pub struct QueryEnginePool {
  engines: Vec<Mutex<QueryEngine>>,
  next: AtomicUsize,
  parse_cache: Mutex<LruCache<String, QuerySpec>>,
}

impl QueryEnginePool {
  /// Create a new pool with the given size.
  /// Recommended size: number of CPU cores.
  pub fn new(size: usize, dialect: SqlDialect) -> Self {
    let size = size.max(1);
    let engines = (0..size)
      .map(|_| Mutex::new(QueryEngine::new(dialect)))
      .collect();
    Self {
      engines,
      next: AtomicUsize::new(0),
      parse_cache: Mutex::new(LruCache::new(std::num::NonZeroUsize::new(1024).unwrap())),
    }
  }

  /// Get the next engine in round-robin fashion.
  pub fn get(&self) -> impl std::ops::Deref<Target = QueryEngine> + '_ {
    let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.engines.len();
    self.engines[idx].lock()
  }

  /// Parse a query, using the cache for repeated queries.
  pub fn parse_query(&self, query: &str) -> Result<QuerySpec, anyhow::Error> {
    // Check cache first
    {
      let mut cache = self.parse_cache.lock();
      if let Some(spec) = cache.get(query) {
        return Ok(spec.clone());
      }
    }

    // Parse with an engine from the pool
    let engine = self.get();
    let spec = engine.parse_query(query)?;

    // Cache the result
    {
      let mut cache = self.parse_cache.lock();
      cache.put(query.to_string(), spec.clone());
    }

    Ok(spec)
  }

  /// Execute a query using a pooled engine.
  pub async fn execute(
    &self,
    query: &str,
    backend: &dyn DatabaseBackend,
  ) -> Result<serde_json::Value, anyhow::Error> {
    let spec = self.parse_query(query)?;
    let sql_filter = spec.filter.as_ref().and_then(|f| f.compiled_sql.as_deref());
    let mut docs = backend
      .list(&spec.table, sql_filter, spec.order_by.as_ref(), spec.limit)
      .await?;

    // JS filtering - use batch evaluation for performance
    if let Some(ref f) = spec.filter {
      if f.compiled_sql.is_none() {
        let engine = self.get();
        docs = engine.js_filter_batch(&docs, &f.js_code)?;
      }
    }

    // JS mapping
    if let Some(ref m) = spec.map {
      let engine = self.get();
      engine.js_map_batch(&docs, m)
    } else {
      Ok(serde_json::to_value(&docs)?)
    }
  }

  /// Get pool size.
  pub fn size(&self) -> usize {
    self.engines.len()
  }
}

impl Default for QueryEnginePool {
  fn default() -> Self {
    Self::new(num_cpus(), SqlDialect::Postgres)
  }
}

/// Helper to get number of CPUs
fn num_cpus() -> usize {
  std::thread::available_parallelism()
    .map(|n| n.get())
    .unwrap_or(4)
}

pub struct QueryEngine {
  runtime: Runtime,
  compiler: QueryCompiler,
}

impl QueryEngine {
  pub fn new(dialect: SqlDialect) -> Self {
    let runtime = Runtime::new().expect("Failed to create JS runtime");
    runtime.set_memory_limit(10 * 1024 * 1024);
    runtime.set_max_stack_size(1024 * 1024);
    Self {
      runtime,
      compiler: QueryCompiler::new(dialect),
    }
  }

  pub fn parse_query(&self, query: &str) -> Result<QuerySpec, anyhow::Error> {
    let ctx = Context::full(&self.runtime)?;
    ctx.with(|ctx| {
      ctx.eval::<(), _>(QUERY_BUILDER_JS)?;
      let result: Value = ctx.eval(query)?;
      let json: String = ctx.eval::<Function, _>("JSON.stringify")?.call((result,))?;
      let v: serde_json::Value = serde_json::from_str(&json)?;

      let table = v["table"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing table"))?
        .into();
      let filter = v["filter"].as_str().map(|js| {
        let compiled = self.compiler.compile_predicate(js);
        FilterSpec {
          js_code: js.into(),
          compiled_sql: match compiled {
            CompiledFilter::Sql(s) | CompiledFilter::Hybrid { sql: s, .. } => Some(s),
            _ => None,
          },
        }
      });
      let map = v["map"].as_str().map(Into::into);
      let order_by = v["orderBy"].as_object().map(|o| OrderBySpec {
        field: o["field"].as_str().unwrap_or("id").into(),
        direction: if o["direction"].as_str() == Some("desc") {
          OrderDirection::Desc
        } else {
          OrderDirection::Asc
        },
      });
      let limit = v["limit"].as_u64().map(|n| n as usize);
      let changes = v["changes"].is_object().then(|| ChangesOptions {
        include_initial: v["changes"]["includeInitial"].as_bool().unwrap_or(false),
      });

      Ok(QuerySpec {
        table,
        filter,
        map,
        order_by,
        limit,
        changes,
      })
    })
  }

  pub async fn execute(
    &self,
    query: &str,
    backend: &dyn DatabaseBackend,
  ) -> Result<serde_json::Value, anyhow::Error> {
    let spec = self.parse_query(query)?;
    let sql_filter = spec.filter.as_ref().and_then(|f| f.compiled_sql.as_deref());
    let mut docs = backend
      .list(&spec.table, sql_filter, spec.order_by.as_ref(), spec.limit)
      .await?;

    if let Some(ref f) = spec.filter {
      if f.compiled_sql.is_none() {
        docs = self.js_filter(&docs, &f.js_code)?;
      }
    }
    if let Some(ref m) = spec.map {
      self.js_map(&docs, m)
    } else {
      Ok(serde_json::to_value(&docs)?)
    }
  }

  fn js_filter(&self, docs: &[Document], code: &str) -> Result<Vec<Document>, anyhow::Error> {
    self.js_filter_batch(docs, code)
  }

  /// Batch filter: compile the function once, call for each document.
  /// This is 10-50x faster than re-parsing for each document.
  ///
  /// The filter function receives the document data merged with metadata:
  /// - `$id`: document ID
  /// - `$created_at`: creation timestamp
  /// - `$updated_at`: update timestamp
  /// - All fields from `data` are accessible directly (e.g., `r.username`)
  pub fn js_filter_batch(
    &self,
    docs: &[Document],
    code: &str,
  ) -> Result<Vec<Document>, anyhow::Error> {
    if docs.is_empty() {
      return Ok(Vec::new());
    }

    let ctx = Context::full(&self.runtime)?;
    ctx.with(|ctx| {
      // Compile the filter function once
      let filter_fn: Function = ctx.eval(format!("({})", code))?;
      let json_parse: Function = ctx.eval("JSON.parse")?;

      let mut out = Vec::with_capacity(docs.len());
      for doc in docs {
        // Merge document metadata with data for filtering
        let filter_obj = self.build_filter_object(doc);
        let obj_str = serde_json::to_string(&filter_obj)?;
        let val: Value = json_parse.call((obj_str,))?;
        if filter_fn.call::<_, bool>((val,))? {
          out.push(doc.clone());
        }
      }
      Ok(out)
    })
  }

  /// Build a merged object for filtering that includes both data fields and metadata.
  /// Metadata fields use $ prefix to avoid conflicts with user data.
  fn build_filter_object(&self, doc: &Document) -> serde_json::Value {
    let mut obj = match &doc.data {
      serde_json::Value::Object(map) => map.clone(),
      _ => serde_json::Map::new(),
    };
    // Add document metadata with $ prefix
    obj.insert("$id".to_string(), serde_json::Value::String(doc.id.to_string()));
    obj.insert(
      "$created_at".to_string(),
      serde_json::Value::String(doc.created_at.to_string()),
    );
    obj.insert(
      "$updated_at".to_string(),
      serde_json::Value::String(doc.updated_at.to_string()),
    );
    serde_json::Value::Object(obj)
  }

  fn js_map(&self, docs: &[Document], code: &str) -> Result<serde_json::Value, anyhow::Error> {
    self.js_map_batch(docs, code)
  }

  /// Batch map: compile the function once, call for each document.
  /// This is 10-50x faster than re-parsing for each document.
  ///
  /// The map function receives the document data merged with metadata:
  /// - `$id`: document ID
  /// - `$created_at`: creation timestamp
  /// - `$updated_at`: update timestamp
  /// - All fields from `data` are accessible directly (e.g., `r.username`)
  pub fn js_map_batch(
    &self,
    docs: &[Document],
    code: &str,
  ) -> Result<serde_json::Value, anyhow::Error> {
    if docs.is_empty() {
      return Ok(serde_json::Value::Array(Vec::new()));
    }

    let ctx = Context::full(&self.runtime)?;
    ctx.with(|ctx| {
      // Compile the map function once
      let map_fn: Function = ctx.eval(format!("({})", code))?;
      let json_parse: Function = ctx.eval("JSON.parse")?;
      let json_stringify: Function = ctx.eval("JSON.stringify")?;

      let mut out = Vec::with_capacity(docs.len());
      for doc in docs {
        // Merge document metadata with data for mapping
        let map_obj = self.build_filter_object(doc);
        let obj_str = serde_json::to_string(&map_obj)?;
        let val: Value = json_parse.call((obj_str,))?;
        let result: Value = map_fn.call((val,))?;
        let result_str: String = json_stringify.call((result,))?;
        out.push(serde_json::from_str(&result_str)?);
      }
      Ok(serde_json::Value::Array(out))
    })
  }
}

impl Default for QueryEngine {
  fn default() -> Self {
    Self::new(SqlDialect::Postgres)
  }
}

const QUERY_BUILDER_JS: &str = r#"
class QueryBuilder {
  constructor() { this._table = null; this._filter = null; this._map = null; this._orderBy = null; this._limit = null; this._changes = null; }
  table(n) { this._table = n; return this; }
  filter(fn) { this._filter = fn.toString(); return this; }
  map(fn) { this._map = fn.toString(); return this; }
  orderBy(f, d) { this._orderBy = { field: f, direction: d || 'asc' }; return this; }
  limit(n) { this._limit = n; return this; }
  changes(o) { this._changes = o || {}; return this; }
  run() { return this; }
  toJSON() { return { table: this._table, filter: this._filter, map: this._map, orderBy: this._orderBy, limit: this._limit, changes: this._changes }; }
}
const db = { table: (n) => new QueryBuilder().table(n) };
"#;
