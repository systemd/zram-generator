/* SPDX-License-Identifier: MIT */

mod config;
mod generator;
mod setup;

use anyhow::Result;
use config::Config;
use std::borrow::Cow;
use std::env;
use std::process::abort;

fn main() -> Result<()> {
    let root: Cow<'static, str> = match env::var("ZRAM_GENERATOR_ROOT") {
        Ok(val) => val.into(),
        Err(env::VarError::NotPresent) => "/".into(),
        Err(_) => abort(),
    };

    Ok(Config::parse(&root)?.run(&root)?)
}
