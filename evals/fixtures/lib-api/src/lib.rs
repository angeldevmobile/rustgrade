//! Fixture: a library whose public API leaks `anyhow` and takes owned refs.

use anyhow::{Context, Result};

pub mod config;

pub fn load(path: &std::path::PathBuf) -> Result<String> {
    std::fs::read_to_string(path).context("reading config")
}

pub fn greet(name: &String) -> String {
    format!("hello {name}")
}
