/* SPDX-License-Identifier: MIT */

use anyhow::{anyhow, Context, Result};
use crate::generator::run_generator;
use crate::setup::run_device_setup;
use ini::ini::{Ini, Properties as IniProperties, SectionIntoIter};
use std::borrow::Cow;
use std::env;
use std::fs;
use std::io::{prelude::*, BufReader};
use std::iter::FromIterator;
use std::path::{Path, PathBuf};


pub struct Device {
    pub name: String,
    pub memory_limit_mb: u64,
    pub zram_fraction: f64,
    pub disksize: u64,
}

impl Device {
    fn new(name: String) -> Device {
        Device {
            name,
            memory_limit_mb: 2 * 1024,
            zram_fraction: 0.25,
            disksize: 0,
        }
    }
}


pub struct Config {
    pub root: Cow<'static, str>,
    pub module: ModuleConfig,
}

pub enum ModuleConfig {
    Generator {
        devices: Vec<Device>,
        output_directory: PathBuf,
    },
    DeviceSetup {
        device: Option<Device>,
        name: String,
    },
}


impl Config {
    pub fn parse() -> Result<Config> {
        let root: Cow<'static, str> = match env::var("ZRAM_GENERATOR_ROOT") {
            Ok(val) => val.into(),
            Err(env::VarError::NotPresent) => "/".into(),
            Err(e) => return Err(e.into()),
        };
        println!("Using {:?} as a root directory", root);

        let mut args = env::args().skip(1);
        let module = match args.next() {
            Some(outdir) => {
                match &outdir[..] {
                    "--setup-device" => {
                        let name = args.next()
                                       .filter(|dev| &dev[0..4] == "zram")
                                       .ok_or_else(|| anyhow!("--setup-device requires device argument"))?;
                        ModuleConfig::DeviceSetup { device: Config::read_device(&root, &name)?, name }
                    }
                    _ =>
                        match (args.next(), args.next(), args.next()) {
                            (Some(_), Some(_), None) |
                            (None, None, None) => {
                                let devices = Config::read_all_devices(&root)?;
                                ModuleConfig::Generator { devices, output_directory: PathBuf::from(outdir) }
                            }
                            _ =>
                                return Err(anyhow!("This program requires 1 or 3 arguments")),
                        }
                }
            }
            None => return Err(anyhow!("This program requires 1 or 3 arguments")),
        };

        Ok(Config { root, module })
    }

    fn read_device(root: &str, name: &str) -> Result<Option<Device>> {
        match Config::read_devices(root)?.find(|(section_name, _)| section_name.as_ref().map(String::as_str) == Some(name)) {
            Some((section_name, section)) => {
                let memtotal_mb = get_total_memory_kb(root)? as f64 / 1024.;
                Config::parse_device(section_name, section, memtotal_mb)
            }
            None => Ok(None),
        }
    }

    fn read_all_devices(root: &str) -> Result<Vec<Device>> {
        let memtotal_mb = get_total_memory_kb(&root)? as f64 / 1024.;
        Result::from_iter(Config::read_devices(root)?.map(|(sn, s)| Config::parse_device(sn, s, memtotal_mb)).map(Result::transpose).flatten())
    }

    fn read_devices(root: &str) -> Result<SectionIntoIter> {
        let path = Path::new(root).join("etc/systemd/zram-generator.conf");
        if !path.exists() {
            println!("No configuration file found.");
            return Ok(Ini::new().into_iter());
        }

        Ok(Ini::load_from_file(&path).with_context(|| {
            format!(
                "Failed to read configuration from {}",
                path.display(),
            )
        })?.into_iter())
    }

    fn parse_device(section_name: Option<String>, section: IniProperties, memtotal_mb: f64) -> Result<Option<Device>> {
        let section_name = section_name.map(Cow::Owned).unwrap_or(Cow::Borrowed("(no title)"));

        if !section_name.starts_with("zram") {
            println!("Ignoring section \"{}\"", section_name);
            return Ok(None);
        }

        let mut dev = Device::new(section_name.into_owned());

        if let Some(val) = section.get("memory-limit") {
            if val == "none" {
                dev.memory_limit_mb = u64::max_value();
            } else {
                dev.memory_limit_mb = val.parse().with_context(|| {
                    format!(
                        "Failed to parse memory-limit \"{}\"",
                        val
                    )
                })?;
            }
        }

        if let Some(val) = section.get("zram-fraction") {
            dev.zram_fraction = val.parse().with_context(|| {
                format!(
                    "Failed to parse zram-fraction \"{}\"",
                    val
                )
            })?;
        }

        println!(
            "Found configuration for {}: memory-limit={}MB zram-fraction={}",
            dev.name, dev.memory_limit_mb, dev.zram_fraction
        );

        if memtotal_mb > dev.memory_limit_mb as f64 {
            println!(
                "{}: system has too much memory ({:.1}MB), limit is {}MB, ignoring.",
                dev.name, memtotal_mb, dev.memory_limit_mb
            );
            Ok(None)
        } else {
            dev.disksize = (dev.zram_fraction * memtotal_mb) as u64 * 1024 * 1024;
            Ok(Some(dev))
        }
    }

    pub fn run(self) -> Result<()> {
        match self.module {
            ModuleConfig::Generator { devices, output_directory } => run_generator(self.root, devices, output_directory),
            ModuleConfig::DeviceSetup { device, name } => run_device_setup(device, name),
        }
    }
}


fn get_total_memory_kb(root: &str) -> Result<u64> {
    let path = Path::new(root).join("proc/meminfo");

    for line in
        BufReader::new(fs::File::open(&path).with_context(|| {
            format!("Failed to read memory information from {}", path.display())
        })?)
        .lines()
    {
        let line = line?;
        let mut fields = line.split_whitespace();
        if let Some("MemTotal:") = fields.next() {
            if let Some(v) = fields.next() {
                return Ok(v.parse()?);
            }
        }
    }

    Err(anyhow!("Couldn't find MemTotal in {}", path.display()))
}
