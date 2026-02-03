//! Redis command handlers

use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use super::config::format_memory_size;
use super::entry::CacheValue;
use super::events::CacheSubscriptionManager;
use super::resp::RespValue;
use super::store::{CacheStore, InMemoryCacheStore};

/// Command execution context
pub struct CommandContext {
  pub store: Arc<InMemoryCacheStore>,
  pub subscriptions: Arc<CacheSubscriptionManager>,
  pub client_id: Uuid,
}

/// Execute a Redis command
pub async fn execute_command(ctx: &CommandContext, cmd: &str, args: &[String]) -> RespValue {
  match cmd {
    "PING" => cmd_ping(args),
    "ECHO" => cmd_echo(args),
    "SET" => cmd_set(ctx, args).await,
    "GET" => cmd_get(ctx, args).await,
    "GETEX" => cmd_getex(ctx, args).await,
    "DEL" => cmd_del(ctx, args).await,
    "EXISTS" => cmd_exists(ctx, args).await,
    "EXPIRE" => cmd_expire(ctx, args).await,
    "PEXPIRE" => cmd_pexpire(ctx, args).await,
    "TTL" => cmd_ttl(ctx, args).await,
    "PTTL" => cmd_pttl(ctx, args).await,
    "PERSIST" => cmd_persist(ctx, args).await,
    "INCR" => cmd_incr(ctx, args).await,
    "DECR" => cmd_decr(ctx, args).await,
    "INCRBY" => cmd_incrby(ctx, args).await,
    "DECRBY" => cmd_decrby(ctx, args).await,
    "MGET" => cmd_mget(ctx, args).await,
    "MSET" => cmd_mset(ctx, args).await,
    "KEYS" => cmd_keys(ctx, args).await,
    "SCAN" => cmd_scan(ctx, args).await,
    "DBSIZE" => cmd_dbsize(ctx).await,
    "FLUSHDB" | "FLUSHALL" => cmd_flushdb(ctx).await,
    "INFO" => cmd_info(ctx, args).await,
    "SELECT" => cmd_select(args),
    "SUBSCRIBE" => cmd_subscribe(ctx, args),
    "PSUBSCRIBE" => cmd_psubscribe(ctx, args),
    "UNSUBSCRIBE" => cmd_unsubscribe(ctx, args),
    "PUNSUBSCRIBE" => cmd_punsubscribe(ctx, args),
    "CLIENT" => cmd_client(args),
    "CONFIG" => cmd_config(args),
    "COMMAND" => cmd_command(),
    "QUIT" => RespValue::ok(),
    _ => RespValue::error(&format!("ERR unknown command '{}'", cmd)),
  }
}

fn cmd_ping(args: &[String]) -> RespValue {
  if args.is_empty() {
    RespValue::pong()
  } else {
    RespValue::bulk(&args[0])
  }
}

fn cmd_echo(args: &[String]) -> RespValue {
  if args.is_empty() {
    RespValue::error("ERR wrong number of arguments for 'echo' command")
  } else {
    RespValue::bulk(&args[0])
  }
}

async fn cmd_set(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.len() < 2 {
    return RespValue::error("ERR wrong number of arguments for 'set' command");
  }

  let key = &args[0];
  let value = CacheValue::from(args[1].clone());
  let mut ttl: Option<Duration> = None;
  let mut nx = false;
  let mut xx = false;

  // Parse options
  let mut i = 2;
  while i < args.len() {
    match args[i].to_uppercase().as_str() {
      "EX" => {
        if i + 1 >= args.len() {
          return RespValue::error("ERR syntax error");
        }
        if let Ok(secs) = args[i + 1].parse::<u64>() {
          ttl = Some(Duration::from_secs(secs));
          i += 2;
        } else {
          return RespValue::error("ERR value is not an integer or out of range");
        }
      }
      "PX" => {
        if i + 1 >= args.len() {
          return RespValue::error("ERR syntax error");
        }
        if let Ok(ms) = args[i + 1].parse::<u64>() {
          ttl = Some(Duration::from_millis(ms));
          i += 2;
        } else {
          return RespValue::error("ERR value is not an integer or out of range");
        }
      }
      "EXAT" => {
        if i + 1 >= args.len() {
          return RespValue::error("ERR syntax error");
        }
        if let Ok(timestamp) = args[i + 1].parse::<u64>() {
          let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
          if timestamp > now {
            ttl = Some(Duration::from_secs(timestamp - now));
          }
          i += 2;
        } else {
          return RespValue::error("ERR value is not an integer or out of range");
        }
      }
      "NX" => {
        nx = true;
        i += 1;
      }
      "XX" => {
        xx = true;
        i += 1;
      }
      "KEEPTTL" => {
        // Keep existing TTL - we'd need to get the current entry first
        i += 1;
      }
      _ => {
        return RespValue::error(&format!("ERR syntax error, unexpected '{}'", args[i]));
      }
    }
  }

  // Check NX/XX conditions
  if nx && ctx.store.exists(key).await {
    return RespValue::null_bulk();
  }
  if xx && !ctx.store.exists(key).await {
    return RespValue::null_bulk();
  }

  match ctx.store.set(key, value, ttl).await {
    Ok(()) => RespValue::ok(),
    Err(e) => RespValue::error(&e.to_string()),
  }
}

async fn cmd_get(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'get' command");
  }

  match ctx.store.get(&args[0]).await {
    Some(entry) => RespValue::bulk(&entry.value.to_resp_string()),
    None => RespValue::null_bulk(),
  }
}

async fn cmd_getex(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'getex' command");
  }

  let key = &args[0];

  // Get the value first
  let entry = match ctx.store.get(key).await {
    Some(e) => e,
    None => return RespValue::null_bulk(),
  };

  // Parse TTL options
  if args.len() > 1 {
    let mut i = 1;
    while i < args.len() {
      match args[i].to_uppercase().as_str() {
        "EX" => {
          if i + 1 >= args.len() {
            return RespValue::error("ERR syntax error");
          }
          if let Ok(secs) = args[i + 1].parse::<u64>() {
            ctx.store.expire(key, Duration::from_secs(secs)).await;
            i += 2;
          } else {
            return RespValue::error("ERR value is not an integer or out of range");
          }
        }
        "PX" => {
          if i + 1 >= args.len() {
            return RespValue::error("ERR syntax error");
          }
          if let Ok(ms) = args[i + 1].parse::<u64>() {
            ctx.store.expire(key, Duration::from_millis(ms)).await;
            i += 2;
          } else {
            return RespValue::error("ERR value is not an integer or out of range");
          }
        }
        "PERSIST" => {
          ctx.store.persist(key).await;
          i += 1;
        }
        _ => {
          return RespValue::error(&format!("ERR syntax error, unexpected '{}'", args[i]));
        }
      }
    }
  }

  RespValue::bulk(&entry.value.to_resp_string())
}

async fn cmd_del(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'del' command");
  }

  let mut deleted = 0i64;
  for key in args {
    if ctx.store.delete(key).await {
      deleted += 1;
    }
  }

  RespValue::integer(deleted)
}

async fn cmd_exists(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'exists' command");
  }

  let mut count = 0i64;
  for key in args {
    if ctx.store.exists(key).await {
      count += 1;
    }
  }

  RespValue::integer(count)
}

async fn cmd_expire(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.len() < 2 {
    return RespValue::error("ERR wrong number of arguments for 'expire' command");
  }

  let secs = match args[1].parse::<u64>() {
    Ok(s) => s,
    Err(_) => return RespValue::error("ERR value is not an integer or out of range"),
  };

  let result = ctx.store.expire(&args[0], Duration::from_secs(secs)).await;
  RespValue::integer(if result { 1 } else { 0 })
}

async fn cmd_pexpire(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.len() < 2 {
    return RespValue::error("ERR wrong number of arguments for 'pexpire' command");
  }

  let ms = match args[1].parse::<u64>() {
    Ok(m) => m,
    Err(_) => return RespValue::error("ERR value is not an integer or out of range"),
  };

  let result = ctx.store.expire(&args[0], Duration::from_millis(ms)).await;
  RespValue::integer(if result { 1 } else { 0 })
}

async fn cmd_ttl(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'ttl' command");
  }

  match ctx.store.ttl(&args[0]).await {
    Some(ttl) => RespValue::integer(ttl),
    None => RespValue::integer(-2), // Key doesn't exist
  }
}

async fn cmd_pttl(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'pttl' command");
  }

  // TTL returns seconds, we need milliseconds
  match ctx.store.ttl(&args[0]).await {
    Some(ttl) if ttl >= 0 => RespValue::integer(ttl * 1000),
    Some(ttl) => RespValue::integer(ttl), // -1 or -2
    None => RespValue::integer(-2),
  }
}

async fn cmd_persist(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'persist' command");
  }

  let result = ctx.store.persist(&args[0]).await;
  RespValue::integer(if result { 1 } else { 0 })
}

async fn cmd_incr(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'incr' command");
  }

  match ctx.store.incr(&args[0], 1).await {
    Ok(val) => RespValue::integer(val),
    Err(e) => RespValue::error(&e.to_string()),
  }
}

async fn cmd_decr(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'decr' command");
  }

  match ctx.store.incr(&args[0], -1).await {
    Ok(val) => RespValue::integer(val),
    Err(e) => RespValue::error(&e.to_string()),
  }
}

async fn cmd_incrby(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.len() < 2 {
    return RespValue::error("ERR wrong number of arguments for 'incrby' command");
  }

  let delta = match args[1].parse::<i64>() {
    Ok(d) => d,
    Err(_) => return RespValue::error("ERR value is not an integer or out of range"),
  };

  match ctx.store.incr(&args[0], delta).await {
    Ok(val) => RespValue::integer(val),
    Err(e) => RespValue::error(&e.to_string()),
  }
}

async fn cmd_decrby(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.len() < 2 {
    return RespValue::error("ERR wrong number of arguments for 'decrby' command");
  }

  let delta = match args[1].parse::<i64>() {
    Ok(d) => d,
    Err(_) => return RespValue::error("ERR value is not an integer or out of range"),
  };

  match ctx.store.incr(&args[0], -delta).await {
    Ok(val) => RespValue::integer(val),
    Err(e) => RespValue::error(&e.to_string()),
  }
}

async fn cmd_mget(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'mget' command");
  }

  let mut results = Vec::with_capacity(args.len());
  for key in args {
    match ctx.store.get(key).await {
      Some(entry) => results.push(RespValue::bulk(&entry.value.to_resp_string())),
      None => results.push(RespValue::null_bulk()),
    }
  }

  RespValue::array(results)
}

async fn cmd_mset(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.len() < 2 || !args.len().is_multiple_of(2) {
    return RespValue::error("ERR wrong number of arguments for 'mset' command");
  }

  for chunk in args.chunks(2) {
    let key = &chunk[0];
    let value = CacheValue::from(chunk[1].clone());
    if let Err(e) = ctx.store.set(key, value, None).await {
      return RespValue::error(&e.to_string());
    }
  }

  RespValue::ok()
}

async fn cmd_keys(ctx: &CommandContext, args: &[String]) -> RespValue {
  let pattern = args.first().map(|s| s.as_str()).unwrap_or("*");
  let keys = ctx.store.keys(pattern).await;
  let items: Vec<RespValue> = keys.into_iter().map(|k| RespValue::bulk(&k)).collect();
  RespValue::array(items)
}

async fn cmd_scan(ctx: &CommandContext, args: &[String]) -> RespValue {
  // Simple SCAN implementation - ignores cursor, returns all matching keys
  let _cursor = args.first().map(|s| s.as_str()).unwrap_or("0");
  let mut pattern = "*";
  let mut count = 10usize;

  // Parse options
  let mut i = 1;
  while i < args.len() {
    match args[i].to_uppercase().as_str() {
      "MATCH" => {
        if i + 1 < args.len() {
          pattern = &args[i + 1];
          i += 2;
        } else {
          i += 1;
        }
      }
      "COUNT" => {
        if i + 1 < args.len() {
          count = args[i + 1].parse().unwrap_or(10);
          i += 2;
        } else {
          i += 1;
        }
      }
      _ => i += 1,
    }
  }

  let keys = ctx.store.keys(pattern).await;
  let limited: Vec<RespValue> = keys
    .into_iter()
    .take(count)
    .map(|k| RespValue::bulk(&k))
    .collect();

  // Return [cursor, [keys...]]
  RespValue::array(vec![
    RespValue::bulk("0"), // Always return cursor 0 (complete)
    RespValue::array(limited),
  ])
}

async fn cmd_dbsize(ctx: &CommandContext) -> RespValue {
  let size = ctx.store.dbsize().await;
  RespValue::integer(size as i64)
}

async fn cmd_flushdb(ctx: &CommandContext) -> RespValue {
  ctx.store.flush().await;
  RespValue::ok()
}

async fn cmd_info(ctx: &CommandContext, args: &[String]) -> RespValue {
  let stats = ctx.store.info().await;
  let section = args.first().map(|s| s.to_lowercase());

  let mut info = String::new();

  // Server section
  if section.is_none() || section.as_deref() == Some("server") {
    info.push_str("# Server\r\n");
    info.push_str("redis_version:7.0.0-squirreldb\r\n");
    info.push_str("tcp_port:6379\r\n");
    info.push_str("\r\n");
  }

  // Memory section
  if section.is_none() || section.as_deref() == Some("memory") {
    info.push_str("# Memory\r\n");
    info.push_str(&format!("used_memory:{}\r\n", stats.memory_used));
    info.push_str(&format!(
      "used_memory_human:{}\r\n",
      format_memory_size(stats.memory_used)
    ));
    info.push_str(&format!("maxmemory:{}\r\n", stats.memory_limit));
    info.push_str(&format!(
      "maxmemory_human:{}\r\n",
      format_memory_size(stats.memory_limit)
    ));
    info.push_str("\r\n");
  }

  // Stats section
  if section.is_none() || section.as_deref() == Some("stats") {
    info.push_str("# Stats\r\n");
    info.push_str(&format!("keyspace_hits:{}\r\n", stats.hits));
    info.push_str(&format!("keyspace_misses:{}\r\n", stats.misses));
    info.push_str(&format!("evicted_keys:{}\r\n", stats.evictions));
    info.push_str(&format!("expired_keys:{}\r\n", stats.expired));
    info.push_str("\r\n");
  }

  // Keyspace section
  if section.is_none() || section.as_deref() == Some("keyspace") {
    info.push_str("# Keyspace\r\n");
    info.push_str(&format!("db0:keys={},expires=0\r\n", stats.keys));
  }

  RespValue::bulk(&info)
}

fn cmd_select(_args: &[String]) -> RespValue {
  // We only support db0, but accept any SELECT command
  RespValue::ok()
}

fn cmd_subscribe(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'subscribe' command");
  }

  let mut results = Vec::new();
  for channel in args {
    let count = ctx.subscriptions.subscribe(ctx.client_id, channel);
    results.push(RespValue::array(vec![
      RespValue::bulk("subscribe"),
      RespValue::bulk(channel),
      RespValue::integer(count as i64),
    ]));
  }

  if results.len() == 1 {
    results.remove(0)
  } else {
    RespValue::array(results)
  }
}

fn cmd_psubscribe(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    return RespValue::error("ERR wrong number of arguments for 'psubscribe' command");
  }

  let mut results = Vec::new();
  for pattern in args {
    let count = ctx.subscriptions.psubscribe(ctx.client_id, pattern);
    results.push(RespValue::array(vec![
      RespValue::bulk("psubscribe"),
      RespValue::bulk(pattern),
      RespValue::integer(count as i64),
    ]));
  }

  if results.len() == 1 {
    results.remove(0)
  } else {
    RespValue::array(results)
  }
}

fn cmd_unsubscribe(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    // Unsubscribe from all
    ctx.subscriptions.remove_client(ctx.client_id);
    return RespValue::array(vec![
      RespValue::bulk("unsubscribe"),
      RespValue::null_bulk(),
      RespValue::integer(0),
    ]);
  }

  let mut results = Vec::new();
  for channel in args {
    let count = ctx.subscriptions.unsubscribe(ctx.client_id, channel);
    results.push(RespValue::array(vec![
      RespValue::bulk("unsubscribe"),
      RespValue::bulk(channel),
      RespValue::integer(count as i64),
    ]));
  }

  if results.len() == 1 {
    results.remove(0)
  } else {
    RespValue::array(results)
  }
}

fn cmd_punsubscribe(ctx: &CommandContext, args: &[String]) -> RespValue {
  if args.is_empty() {
    ctx.subscriptions.remove_client(ctx.client_id);
    return RespValue::array(vec![
      RespValue::bulk("punsubscribe"),
      RespValue::null_bulk(),
      RespValue::integer(0),
    ]);
  }

  let mut results = Vec::new();
  for pattern in args {
    let count = ctx.subscriptions.punsubscribe(ctx.client_id, pattern);
    results.push(RespValue::array(vec![
      RespValue::bulk("punsubscribe"),
      RespValue::bulk(pattern),
      RespValue::integer(count as i64),
    ]));
  }

  if results.len() == 1 {
    results.remove(0)
  } else {
    RespValue::array(results)
  }
}

fn cmd_client(args: &[String]) -> RespValue {
  let subcommand = args.first().map(|s| s.to_uppercase());
  match subcommand.as_deref() {
    Some("SETNAME") => RespValue::ok(),
    Some("GETNAME") => RespValue::null_bulk(),
    Some("LIST") => RespValue::bulk(""),
    Some("ID") => RespValue::integer(1),
    _ => RespValue::ok(),
  }
}

fn cmd_config(args: &[String]) -> RespValue {
  let subcommand = args.first().map(|s| s.to_uppercase());
  match subcommand.as_deref() {
    Some("GET") => {
      // Return empty array for config get
      RespValue::array(vec![])
    }
    Some("SET") => RespValue::ok(),
    _ => RespValue::ok(),
  }
}

fn cmd_command() -> RespValue {
  // Return minimal command info
  RespValue::array(vec![])
}
