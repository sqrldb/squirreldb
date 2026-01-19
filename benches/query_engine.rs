//! Query engine benchmarks for SquirrelDB.
//!
//! Run with: cargo bench --bench query_engine

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde_json::json;
use squirreldb::db::{DatabaseBackend, SqlDialect, SqliteBackend};
use squirreldb::query::{QueryCompiler, QueryEnginePool};
use tokio::runtime::Runtime;

fn create_runtime() -> Runtime {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap()
}

fn bench_query_parse(c: &mut Criterion) {
  let mut group = c.benchmark_group("query_parse");

  let pool = QueryEnginePool::new(4, SqlDialect::Sqlite);

  group.bench_function("simple", |b| {
    b.iter(|| {
      black_box(pool.parse_query(r#"db.table("users").run()"#).unwrap());
    });
  });

  group.bench_function("with_filter", |b| {
    b.iter(|| {
      black_box(
        pool
          .parse_query(r#"db.table("users").filter(u => u.age > 30).run()"#)
          .unwrap(),
      );
    });
  });

  group.bench_function("with_filter_and_limit", |b| {
    b.iter(|| {
      black_box(
        pool
          .parse_query(r#"db.table("users").filter(u => u.age > 30).limit(10).run()"#)
          .unwrap(),
      );
    });
  });

  group.bench_function("with_order", |b| {
    b.iter(|| {
      black_box(
        pool
          .parse_query(r#"db.table("users").orderBy("name", "asc").run()"#)
          .unwrap(),
      );
    });
  });

  group.bench_function("complex", |b| {
    b.iter(|| {
      black_box(
        pool
          .parse_query(
            r#"db.table("users").filter(u => u.age > 25 && u.active).orderBy("score", "desc").limit(50).run()"#,
          )
          .unwrap(),
      );
    });
  });

  group.bench_function("with_map", |b| {
    b.iter(|| {
      black_box(
        pool
          .parse_query(r#"db.table("users").map(u => ({name: u.name, age: u.age})).run()"#)
          .unwrap(),
      );
    });
  });

  group.finish();
}

fn bench_query_parse_cached(c: &mut Criterion) {
  let mut group = c.benchmark_group("query_parse_cached");

  let pool = QueryEnginePool::new(4, SqlDialect::Sqlite);
  let query = r#"db.table("users").filter(u => u.age > 30).limit(10).run()"#;

  // Prime the cache
  pool.parse_query(query).unwrap();

  group.bench_function("cache_hit", |b| {
    b.iter(|| {
      black_box(pool.parse_query(query).unwrap());
    });
  });

  group.finish();
}

fn bench_filter_compilation(c: &mut Criterion) {
  let mut group = c.benchmark_group("filter_compilation");

  let postgres_compiler = QueryCompiler::new(SqlDialect::Postgres);
  let sqlite_compiler = QueryCompiler::new(SqlDialect::Sqlite);

  group.bench_function("postgres_simple_eq", |b| {
    b.iter(|| {
      black_box(postgres_compiler.compile_predicate(r#"doc => doc.status === "active""#));
    });
  });

  group.bench_function("sqlite_simple_eq", |b| {
    b.iter(|| {
      black_box(sqlite_compiler.compile_predicate(r#"doc => doc.status === "active""#));
    });
  });

  group.bench_function("postgres_numeric_comparison", |b| {
    b.iter(|| {
      black_box(postgres_compiler.compile_predicate("doc => doc.age > 30"));
    });
  });

  group.bench_function("postgres_logical_and", |b| {
    b.iter(|| {
      black_box(postgres_compiler.compile_predicate("doc => doc.age > 30 && doc.active"));
    });
  });

  group.bench_function("postgres_logical_or", |b| {
    b.iter(|| {
      black_box(
        postgres_compiler
          .compile_predicate(r#"doc => doc.status === "pending" || doc.status === "active""#),
      );
    });
  });

  group.bench_function("postgres_nested_field", |b| {
    b.iter(|| {
      black_box(postgres_compiler.compile_predicate(r#"doc => doc.address.city === "NYC""#));
    });
  });

  group.bench_function("js_fallback_method_call", |b| {
    b.iter(|| {
      black_box(postgres_compiler.compile_predicate("doc => doc.tags.includes('vip')"));
    });
  });

  group.finish();
}

fn bench_query_execute(c: &mut Criterion) {
  let rt = create_runtime();

  let mut group = c.benchmark_group("query_execute");

  // Test with 100 documents
  let pool = std::sync::Arc::new(QueryEnginePool::new(4, SqlDialect::Sqlite));

  // Setup backend with 100 documents
  let backend = rt.block_on(async {
    let b = SqliteBackend::in_memory().await.unwrap();
    b.init_schema().await.unwrap();
    for i in 0..100 {
      b.insert(
        "users",
        json!({
          "name": format!("User {}", i),
          "age": 20 + (i % 50),
          "active": i % 2 == 0,
          "score": (i * 7) % 100,
          "tags": ["tag1", "tag2"],
          "address": {
            "city": if i % 3 == 0 { "NYC" } else { "LA" },
            "zip": format!("{:05}", 10000 + i)
          }
        }),
      )
      .await
      .unwrap();
    }
    b
  });

  group.bench_function("select_all", |b| {
    let pool = pool.clone();
    b.iter(|| {
      rt.block_on(async {
        black_box(
          pool
            .execute(r#"db.table("users").run()"#, &backend)
            .await
            .unwrap(),
        );
      });
    });
  });

  group.bench_function("sql_filter", |b| {
    let pool = pool.clone();
    b.iter(|| {
      rt.block_on(async {
        // This filter compiles to SQL
        black_box(
          pool
            .execute(
              r#"db.table("users").filter(u => u.age > 40).run()"#,
              &backend,
            )
            .await
            .unwrap(),
        );
      });
    });
  });

  group.bench_function("with_limit", |b| {
    let pool = pool.clone();
    b.iter(|| {
      rt.block_on(async {
        black_box(
          pool
            .execute(r#"db.table("users").limit(10).run()"#, &backend)
            .await
            .unwrap(),
        );
      });
    });
  });

  group.bench_function("with_order", |b| {
    let pool = pool.clone();
    b.iter(|| {
      rt.block_on(async {
        black_box(
          pool
            .execute(
              r#"db.table("users").orderBy("score", "desc").run()"#,
              &backend,
            )
            .await
            .unwrap(),
        );
      });
    });
  });

  group.finish();
}

fn bench_pool_contention(c: &mut Criterion) {
  let mut group = c.benchmark_group("pool_contention");

  for pool_size in [1, 2, 4].iter() {
    group.throughput(Throughput::Elements(8));

    group.bench_with_input(
      BenchmarkId::new("sequential_parses", pool_size),
      pool_size,
      |b, &pool_size| {
        let pool = QueryEnginePool::new(pool_size, SqlDialect::Sqlite);

        b.iter(|| {
          // Run 8 queries sequentially
          for i in 0..8 {
            let query = format!(r#"db.table("users").filter(u => u.index === {}).run()"#, i);
            black_box(pool.parse_query(&query).unwrap());
          }
        });
      },
    );
  }

  group.finish();
}

criterion_group!(
  benches,
  bench_query_parse,
  bench_query_parse_cached,
  bench_filter_compilation,
  bench_query_execute,
  bench_pool_contention,
);

criterion_main!(benches);
