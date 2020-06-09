/* SPDX-License-Identifier: MIT */

mod config;
mod generator;
mod setup;

use anyhow::Result;
use config::Config;

fn main() -> Result<()> {
    Ok(Config::parse()?.run()?)
}
