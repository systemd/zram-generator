/* SPDX-License-Identifier: MIT */

mod config;
mod generator;
mod kernlog;
mod setup;

use anyhow::Result;
use log::{info, LevelFilter};
use std::borrow::Cow;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug)]
enum Opts {
    /// Generate units into the directory
    GenerateUnits(String),
    /// Set up a single device
    SetupDevice(String),
    /// Reset (destroy) a device
    ResetDevice(String),
}

#[rustfmt::skip]
fn usage() -> ! {
    eprintln!(
        "Usage:\t{0} dir1 [dir2 [dir3]]     # Generate systemd units\n\
               \t{0} --setup-device device  # Set up a single device\n\
               \t{0} --reset-device device  # Reset (destroy) a device",
        env::args().next().as_deref().unwrap_or("zram-generator")
    );
    eprintln!();
    eprintln!("Uses {}.", setup::SYSTEMD_MAKEFS_COMMAND);
    std::process::exit(1);
}

fn get_opts() -> Opts {
    let mut opts = getopts::Options::new();
    opts.optopt("", "setup-device", "", "device");
    opts.optopt("", "reset-device", "", "device");

    let mut ret = match opts.parse(env::args_os().skip(1)) {
        Ok(ret) => ret,
        Err(err) => {
            eprintln!("{}", err);
            usage()
        }
    };
    match (
        ret.opt_str("setup-device"),
        ret.opt_str("reset-device"),
        ret.free.len(),
    ) {
        (None, None, 1..=3) => Opts::GenerateUnits(ret.free.swap_remove(0)),
        (Some(setup), None, 0) => Opts::SetupDevice(setup),
        (None, Some(reset), 0) => Opts::ResetDevice(reset),
        _ => usage(),
    }
}

fn main() -> Result<()> {
    let (root, have_env_var, log_level) = match env::var_os("ZRAM_GENERATOR_ROOT") {
        Some(val) => (PathBuf::from(val).into(), true, LevelFilter::Trace),
        None => (Cow::from(Path::new("/")), false, LevelFilter::Info),
    };

    let _ = kernlog::init_with_level(log_level);

    let kernel_override = || match config::kernel_zram_option(&root) {
        Some(false) => {
            info!("Disabled by kernel cmdline option, exiting.");
            std::process::exit(0);
        }
        None => false,
        Some(true) => true,
    };

    match get_opts() {
        Opts::GenerateUnits(target) => {
            let devices = config::read_all_devices(&root, kernel_override())?;
            let output_directory = PathBuf::from(target);
            generator::run_generator(&devices, &output_directory, have_env_var)
        }
        Opts::SetupDevice(dev) => {
            let device = config::read_device(&root, kernel_override(), &dev)?;
            setup::run_device_setup(device, &dev)
        }
        Opts::ResetDevice(dev) => {
            // We don't read the config here, so that it's possible to remove a device
            // even after the config has been removed.
            setup::run_device_reset(&dev)
        }
    }
}
