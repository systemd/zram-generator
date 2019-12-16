/* SPDX-License-Identifier: MIT */

use anyhow::{anyhow, Context, Result};
use crate::generator::run_generator;
use ini::Ini;
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
    pub devices: Vec<Device>,
    pub module: ModuleConfig,
}

pub enum ModuleConfig {
    Generator { output_directory: PathBuf },
}


impl Config {
    pub fn parse() -> Result<Config> {
        let root: Cow<'static, str> = match env::var("ZRAM_GENERATOR_ROOT") {
            Ok(val) => val.into(),
            Err(env::VarError::NotPresent) => "/".into(),
            Err(e) => return Err(e.into()),
        };
        println!("Using {:?} as a root directory", root);

        let args: Vec<String> = env::args().collect();
        let output_directory = match args.len() {
            2 | 4 => PathBuf::from(&args[1]),
            _ => return Err(anyhow!("This program requires 1 or 3 arguments")),
        };

        let devices = Config::read_devices(&root)?;
        let module = ModuleConfig::Generator { output_directory };
        Ok(Config { root, devices, module })
    }

    fn read_devices(root: &str) -> Result<Vec<Device>> {
        let path = Path::new(root).join("etc/systemd/zram-generator.conf");
        if !path.exists() {
            println!("No configuration file found.");
            return Ok(vec![]);
        }

        let memtotal_mb = get_total_memory_kb(&root)? as f64 / 1024.;

        Result::from_iter(
            Ini::load_from_file(&path)
                .with_context(|| format!("Failed to read configuration from {}", path.display()))?
                .into_iter()
                .map(|(section_name, section)| {
                    let section_name = section_name
                        .map(Cow::Owned)
                        .unwrap_or(Cow::Borrowed("(no title)"));

                    if !section_name.starts_with("zram") {
                        println!("Ignoring section \"{}\"", section_name);
                        return Ok(None);
                    }

                    let mut dev = Device::new(section_name.into_owned());

                    if let Some(val) = section.get("memory-limit") {
                        if val == "none" {
                            dev.memory_limit_mb = u64::max_value();
                        } else {
                            dev.memory_limit_mb = val.parse().map_err(|e| {
                                anyhow!("Failed to parse memory-limit \"{}\": {}", val, e)
                            })?;
                        }
                    }

                    if let Some(val) = section.get("zram-fraction") {
                        dev.zram_fraction = val.parse().map_err(|e| {
                            anyhow!("Failed to parse zram-fraction \"{}\": {}", val, e)
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
                })
                .map(Result::transpose)
                .flatten(),
        )
    }

    pub fn run(self) -> Result<()> {
        match self.module {
            ModuleConfig::Generator { output_directory } => {
                run_generator(self.root, self.devices, output_directory)
            }
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
