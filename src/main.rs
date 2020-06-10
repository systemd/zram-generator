/* SPDX-License-Identifier: MIT */

mod config;
mod generator;
mod setup;

use anyhow::{anyhow, Result};
use std::borrow::Cow;
use std::env;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

fn zram_name(name: &str) -> Result<String> {
    if name.starts_with("zram") {
        Ok(name.to_owned())
    } else {
        Err(anyhow!("device name must start with \"zram\""))
    }
}

#[derive(Debug, StructOpt)]
#[structopt(about = "Systemd generator for zram swap devices.")]
enum Opts {
    /// Set up a new zram swap device defined in /etc/systemd/zram-generator.conf
    Setup {
        /// The name of the ini section the device is defined in
        #[structopt(parse(try_from_str = zram_name))]
        name: String,
    },
    /// Generate the systemd service file
    Generate {
        /// The normal directory to place generated units
        #[structopt(parse(from_os_str))]
        normal_directory: PathBuf,
        /// The early directory for generated units which take precedence over all normal units;
        /// if provided, <late-directory> must also be provided.
        #[structopt(parse(from_os_str))]
        early_directory: Option<PathBuf>,
        /// The late directory for generated units which do not override any other units;
        /// if provided, <early-directory> must also be provided.
        #[structopt(parse(from_os_str))]
        late_directory: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let root: Cow<'static, str> = match env::var("ZRAM_GENERATOR_ROOT") {
        Ok(val) => val.into(),
        Err(env::VarError::NotPresent) => "/".into(),
        Err(e) => return Err(e.into()),
    };
    let root = Path::new(&root[..]);

    let opts = Opts::from_args();

    match opts {
        Opts::Setup { name } => {
            let device = config::read_device(&root, &name)?;
            Ok(setup::run_device_setup(device, &name)?)
        }
        Opts::Generate {
            normal_directory,
            early_directory,
            late_directory,
        } => {
            if early_directory.xor(late_directory) != None {
                return Err(anyhow!(
                    "both <early-directory> and <late-directory> must be provided, or neither one"
                ));
            }
            let devices = config::read_all_devices(&root)?;
            Ok(generator::run_generator(
                &root,
                &devices,
                &normal_directory,
            )?)
        }
    }
}
