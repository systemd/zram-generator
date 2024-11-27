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
fn command() -> clap::Command {
    clap::command!()
        .override_usage("\
            \tzram-generator --setup-device <device>\n\
            \tzram-generator --reset-device <device>\n\
            \tzram-generator dir1 [dir2 dir3]\
        ")
        .arg(
            clap::arg!(--"setup-device" <device> "Set up a single device")
                .conflicts_with("reset-device")
        )
        .arg(
            clap::arg!(--"reset-device" <device> "Reset (destroy) a device")
        )
        .arg(
            clap::arg!([dir] "Target directory to write output to and two optional\n\
                              unused directories to satisfy systemd.generator(5)")
                .num_args(1..=3)
                .conflicts_with_all(["setup-device", "reset-device"])
                .required_unless_present_any(["setup-device", "reset-device"])
        )
        .after_help(setup::AFTER_HELP)
}

fn get_opts() -> Opts {
    let opts = command().get_matches();

    if let Some(val) = opts.get_one::<&str>("setup-device") {
        Opts::SetupDevice(val.to_string())
    } else if let Some(val) = opts.get_one::<&str>("reset-device") {
        Opts::ResetDevice(val.to_string())
    } else {
        let val = opts.get_one::<&str>("dir").expect("clap invariant");
        Opts::GenerateUnits(val.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_app() {
        command().debug_assert();
    }

    #[test]
    fn parse_setup_device() {
        let m = command().get_matches_from(vec!["prog", "--setup-device", "/dev/zram1"]);
        assert_eq!(m.get_one::<String>("setup-device").unwrap(), "/dev/zram1");
    }

    #[test]
    fn parse_reset_device() {
        let m = command().get_matches_from(vec!["prog", "--reset-device", "/dev/zram1"]);
        assert_eq!(m.get_one::<String>("reset-device").unwrap(), "/dev/zram1");
    }

    #[test]
    fn parse_with_dir() {
        let m = command().get_matches_from(vec!["prog", "/dir1"]);
        assert!(m.get_one::<String>("setup-device").is_none());
        assert!(m.get_one::<String>("reset-device").is_none());
        assert_eq!(m.get_one::<String>("dir").unwrap(), "/dir1");
    }

    #[test]
    fn parse_with_dirs() {
        let m = command().get_matches_from(vec!["prog", "/dir1", "/dir2", "/dir3"]);
        assert!(m.get_one::<String>("setup-device").is_none());
        assert!(m.get_one::<String>("reset-device").is_none());
        assert_eq!(m.get_one::<String>("dir").unwrap(), "/dir1");
    }
}
