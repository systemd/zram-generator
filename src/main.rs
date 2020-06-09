/* SPDX-License-Identifier: MIT */

mod config;
mod generator;
mod setup;

use crate::generator::run_generator;
use crate::setup::run_device_setup;
use anyhow::{anyhow, Result};
use config::Config;
use std::borrow::Cow;
use std::env;
use std::path::PathBuf;
use std::process::abort;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "Systemd generator for zram swap devices.")]
struct Opts {
    /// Set up a single device
    #[structopt(long)]
    setup_device: bool,

    arg: String,
    extra: Vec<String>,
}

fn get_opts() -> Result<Opts> {
    let opts = Opts::from_args();
    println!("{:?}", opts);

    if opts.setup_device && !opts.extra.is_empty() {
        return Err(anyhow!("--setup-device accepts exactly one argument"));
    }

    if !opts.setup_device && !opts.extra.is_empty() && opts.extra.len() != 2 {
        return Err(anyhow!("This program requires 1 or 3 arguments"));
    }

    Ok(opts)
}

fn main() -> Result<()> {
    let root: Cow<'static, str> = match env::var("ZRAM_GENERATOR_ROOT") {
        Ok(val) => val.into(),
        Err(env::VarError::NotPresent) => "/".into(),
        Err(_) => abort(),
    };

    let opts = get_opts()?;

    if opts.setup_device {
        let device = Config::read_device(&root, &opts.arg)?;
        Ok(run_device_setup(device, &opts.arg)?)
    } else {
        let devices = Config::read_all_devices(&root)?;
        let output_directory = PathBuf::from(opts.arg);
        Ok(run_generator(&root, devices, &output_directory)?)
    }
}
