//! Database operation benchmarks for SquirrelDB.
//!
//! Run with: cargo bench --bench database

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde_json::json;
use squirreldb::db::{DatabaseBackend, SqliteBackend};
use tokio::runtime::Runtime;
use uuid::Uuid;

fn create_runtime() -> Runtime {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap()
}

fn bench_insert(c: &mut Criterion) {
  let rt = create_runtime();

  let mut group = c.benchmark_group("insert");
  group.throughput(Throughput::Elements(1));

  // Pre-create backend once for the benchmark
  let backend = rt.block_on(async {
    let b = SqliteBackend::in_memory().await.unwrap();
    b.init_schema().await.unwrap();
    b
  });

  group.bench_function("simple_document", |b| {
    b.iter(|| {
      rt.block_on(async {
        black_box(
          backend
            .insert("users", json!({"name": "Alice", "age": 30}))
            .await
            .unwrap(),
        );
      });
    });
  });

  group.bench_function("complex_document", |b| {
    b.iter(|| {
      rt.block_on(async {
        black_box(
          backend
            .insert(
              "orders",
              json!({
                "user_id": "123",
                "items": [
                  {"product": "Widget", "quantity": 5, "price": 9.99},
                  {"product": "Gadget", "quantity": 2, "price": 19.99},
                  {"product": "Gizmo", "quantity": 1, "price": 29.99}
                ],
                "shipping": {
                  "address": {
                    "street": "123 Main St",
                    "city": "Anytown",
                    "state": "CA",
                    "zip": "12345"
                  },
                  "method": "express"
                },
                "total": 119.92
              }),
            )
            .await
            .unwrap(),
        );
      });
    });
  });

  group.finish();
}

fn bench_insert_batch(c: &mut Criterion) {
  let rt = create_runtime();

  let mut group = c.benchmark_group("insert_batch");

  for size in [10, 50].iter() {
    group.throughput(Throughput::Elements(*size as u64));

    group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
      b.iter(|| {
        rt.block_on(async {
          let backend = SqliteBackend::in_memory().await.unwrap();
          backend.init_schema().await.unwrap();
          for i in 0..size {
            backend
              .insert("batch", json!({"index": i, "data": format!("item_{}", i)}))
              .await
              .unwrap();
          }
        });
      });
    });
  }

  group.finish();
}

fn bench_get(c: &mut Criterion) {
  let rt = create_runtime();

  let mut group = c.benchmark_group("get");
  group.throughput(Throughput::Elements(1));

  // Setup: create backend with one document
  let (backend, doc_id) = rt.block_on(async {
    let b = SqliteBackend::in_memory().await.unwrap();
    b.init_schema().await.unwrap();
    let doc = b.insert("users", json!({"name": "Bob"})).await.unwrap();
    (b, doc.id)
  });

  group.bench_function("existing_document", |b| {
    b.iter(|| {
      rt.block_on(async {
        black_box(backend.get("users", doc_id).await.unwrap());
      });
    });
  });

  group.bench_function("nonexistent_document", |b| {
    b.iter(|| {
      rt.block_on(async {
        black_box(backend.get("users", Uuid::new_v4()).await.unwrap());
      });
    });
  });

  group.finish();
}

fn bench_update(c: &mut Criterion) {
  let rt = create_runtime();

  let mut group = c.benchmark_group("update");
  group.throughput(Throughput::Elements(1));

  // Setup: create backend with one document
  let (backend, doc_id) = rt.block_on(async {
    let b = SqliteBackend::in_memory().await.unwrap();
    b.init_schema().await.unwrap();
    let doc = b
      .insert("users", json!({"name": "Charlie", "version": 0}))
      .await
      .unwrap();
    (b, doc.id)
  });

  let mut version = 0i32;
  group.bench_function("existing_document", |b| {
    b.iter(|| {
      version += 1;
      rt.block_on(async {
        black_box(
          backend
            .update(
              "users",
              doc_id,
              json!({"name": "Charlie", "version": version}),
            )
            .await
            .unwrap(),
        );
      });
    });
  });

  group.finish();
}

fn bench_delete(c: &mut Criterion) {
  let rt = create_runtime();

  let mut group = c.benchmark_group("delete");
  group.throughput(Throughput::Elements(1));

  let backend = rt.block_on(async {
    let b = SqliteBackend::in_memory().await.unwrap();
    b.init_schema().await.unwrap();
    b
  });

  group.bench_function("existing_document", |b| {
    b.iter(|| {
      rt.block_on(async {
        // Insert then delete to measure delete time
        let doc = backend
          .insert("users", json!({"name": "DeleteMe"}))
          .await
          .unwrap();
        black_box(backend.delete("users", doc.id).await.unwrap());
      });
    });
  });

  group.finish();
}

fn bench_list(c: &mut Criterion) {
  let rt = create_runtime();

  let mut group = c.benchmark_group("list");

  // Benchmark listing with different collection sizes
  for size in [10, 100].iter() {
    group.throughput(Throughput::Elements(*size as u64));

    // Create backend with data
    let backend = rt.block_on(async {
      let b = SqliteBackend::in_memory().await.unwrap();
      b.init_schema().await.unwrap();
      for i in 0..*size {
        b.insert("items", json!({"index": i})).await.unwrap();
      }
      b
    });

    group.bench_with_input(BenchmarkId::new("all_documents", size), size, |b, _| {
      b.iter(|| {
        rt.block_on(async {
          black_box(backend.list("items", None, None, None, None).await.unwrap());
        });
      });
    });
  }

  group.finish();
}

fn bench_list_with_filter(c: &mut Criterion) {
  let rt = create_runtime();

  let mut group = c.benchmark_group("list_filtered");

  // Pre-populate with 100 documents
  let backend = rt.block_on(async {
    let b = SqliteBackend::in_memory().await.unwrap();
    b.init_schema().await.unwrap();
    for i in 0..100 {
      b.insert(
        "products",
        json!({"name": format!("Product {}", i), "price": (i % 100) as f64}),
      )
      .await
      .unwrap();
    }
    b
  });

  group.bench_function("sql_filter", |b| {
    b.iter(|| {
      rt.block_on(async {
        // SQL filter: price > 50
        black_box(
          backend
            .list(
              "products",
              Some("CAST(json_extract(data, '$.price') AS REAL) > 50"),
              None,
              None,
              None,
            )
            .await
            .unwrap(),
        );
      });
    });
  });

  group.finish();
}

fn bench_list_with_limit(c: &mut Criterion) {
  let rt = create_runtime();

  let mut group = c.benchmark_group("list_limited");

  // Benchmark with limits on collections
  let collection_size = 500;

  // Create backend with data
  let backend = rt.block_on(async {
    let b = SqliteBackend::in_memory().await.unwrap();
    b.init_schema().await.unwrap();
    for i in 0..collection_size {
      b.insert("large", json!({"index": i})).await.unwrap();
    }
    b
  });

  for limit in [10, 50, 100].iter() {
    group.throughput(Throughput::Elements(*limit as u64));
    group.bench_with_input(BenchmarkId::from_parameter(limit), limit, |b, &limit| {
      b.iter(|| {
        rt.block_on(async {
          black_box(
            backend
              .list("large", None, None, Some(limit), None)
              .await
              .unwrap(),
          );
        });
      });
    });
  }

  group.finish();
}

criterion_group!(
  benches,
  bench_insert,
  bench_insert_batch,
  bench_get,
  bench_update,
  bench_delete,
  bench_list,
  bench_list_with_filter,
  bench_list_with_limit,
);

criterion_main!(benches);
