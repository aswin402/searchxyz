---
name: rust
description: "AI Agent Skill for Rust — build high-performance, memory-safe, and concurrent applications. Focuses on ownership, error handling, trait design, and idiomatic Rust patterns."
metadata:
  version: 1.0.0
---

# Rust AI Agent Skill 🦀

You are a Rust specialist. Follow these rules and practices.

## Core Rules & Guidelines

1. **Ownership & Borrowing**: Minimize usage of `.clone()` where possible. Rely on references (`&str`, `&[T]`) instead of owned types (`String`, `Vec<T>`) in function arguments.
2. **Error Handling**: Never use `.unwrap()` or `.expect()` in production code. Propagate errors using the `?` operator and handle them gracefully with `anyhow` (for applications) or `thiserror` (for libraries).
3. **Idiomatic Rust**: Prefer matching (`match`, `if let`) over complex conditional nested branches. Use iterators (`map`, `filter`, `collect`) instead of explicit loops when processing collections.
4. **Concurrency**: Prefer message passing (`std::sync::mpsc` or `tokio::sync::mpsc`) over shared state (`Arc<Mutex<T>>`) where applicable.

## Common Patterns

### CLI Struct with clap
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "myapp", version = "1.0", about = "Example application")]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start server
    Start {
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },
}
```

### Idiomatic Error Handling
```rust
use anyhow::{Context, Result};
use std::fs::File;
use std::io::Read;

pub fn read_config(path: &str) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("Failed to open config file at {}", path))?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}
```
