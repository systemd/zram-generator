/* SPDX-License-Identifier: MIT */

mod config;
mod generator;
mod setup;

use anyhow::{anyhow, Result};
use std::borrow::Cow;
use std::env;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "Systemd generator for zram swap devices.")]
struct Opts {
    /// Set up a single device
    #[structopt(long)]
    setup_device: bool,

    /// Reset (destroy) a device
    #[structopt(long)]
    reset_device: bool,

    arg: String,
    extra: Vec<String>,
}

fn get_opts() -> Result<Opts> {
    let opts = Opts::from_args();
    println!("{:?}", opts);

    if opts.setup_device {
        if !opts.extra.is_empty() {
            return Err(anyhow!("--setup-device accepts exactly one argument"));
        }

        if opts.reset_device {
            return Err(anyhow!(
                "--setup-device cannot be combined with --reset-device"
            ));
        }
    }

    if opts.reset_device && !opts.extra.is_empty() {
        return Err(anyhow!("--reset-device accepts exactly one argument"));
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
        Err(e) => return Err(e.into()),
    };
    let root = Path::new(&root[..]);

    let opts = get_opts()?;

    if opts.setup_device {
        let device = config::read_device(&root, &opts.arg)?;
        Ok(setup::run_device_setup(device, &opts.arg)?)
    } else if opts.reset_device {
        // We don't read the config here, so that it's possible to remove a device
        // even after the config has been removed.
        Ok(setup::run_device_reset(&opts.arg)?)
    } else {
        let devices = config::read_all_devices(&root)?;
        let output_directory = PathBuf::from(opts.arg);
        Ok(generator::run_generator(
            &root,
            &devices,
            &output_directory,
        )?)
    }
}
