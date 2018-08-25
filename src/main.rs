/* SPDX-License-Identifier: MIT */

#[macro_use]
extern crate failure;
extern crate ini;

use failure::Error;
use ini::Ini;
use std::env;
use std::fmt;
use std::fs;
use std::io::prelude::*;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::result;

pub trait ResultExt<T, E>: failure::ResultExt<T, E>
where
    E: fmt::Display,
{
    fn with_path<P: AsRef<Path>>(self, path: P) -> result::Result<T, failure::Context<String>>
    where
        Self: Sized,
    {
        self.with_context(|e| format!("{}: {}", path.as_ref().display(), e))
    }
}

impl<T, E: fmt::Display> ResultExt<T, E> for result::Result<T, E> where
    result::Result<T, E>: failure::ResultExt<T, E>
{}

fn make_symlink(dst: &str, src: &Path) -> Result<(), Error> {
    let parent = src.parent().unwrap();

    let _ = fs::create_dir(&parent);
    symlink(dst, src).with_path(src)?;
    Ok(())
}

fn virtualization_container() -> Result<bool, Error> {
    let output = match Command::new("systemd-detect-virt").arg("--container").output() {
        Ok(ok) => ok,
        Err(e) => return Err(format_err!("systemd-detect-virt call failed: {}", e)),
    };
    return Ok(output.status.success());
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let config = match Config::new(&args) {
        Ok(ok) => ok,
        Err(e) => {
            println!("{}", e);
            std::process::exit(1);
        },
    };

    if config.devices.len() == 0 {
        println!("No devices configured, exiting.");
        std::process::exit(0);
    }

    if let Err(e) = run(config) {
        println!("{}", e);
        std::process::exit(2);
    }
}

struct Device {
    name: String,
    memory_limit_mb: u64,
    zram_fraction: f64,
}

impl Device {
    fn new(name: String) -> Device {
        Device {
            name,
            memory_limit_mb: 2 * 1024,
            zram_fraction: 0.25,
        }
    }
}

struct Config {
    output_directory: PathBuf,

    devices: Vec<Device>,
}

impl Config {
    fn new(args: &[String]) -> Result<Config, Error> {
        let output_directory = match args.len() {
            2 | 4 => PathBuf::from(&args[1]),
            _ => return Err(failure::err_msg("This program requires 1 or 3 arguments")),
        };

        let devices = Vec::new();

        let mut config = Config {
            output_directory,
            devices,
        };

        config.read()?;

        Ok(config)
    }

    fn read(&mut self) -> Result<bool, Error> {
        let path = Path::new("/etc/systemd/zram-generator.conf");
        if !path.exists() {
            println!("No configuration file found.");
            return Ok(false);
        }

        let conf = Ini::load_from_file(path).with_path(path)?;

        let no_title = "(no title)".into();
        for (section_name, section) in conf.iter() {
            let section_name = section_name.as_ref().unwrap_or(&no_title);

            if !section_name.starts_with("zram") {
                println!("Ignoring section \"{}\"", section_name);
                continue;
            }

            let mut dev = Device::new(section_name.to_string());

            if let Some(val) = section.get("memory-limit") {
                dev.memory_limit_mb = val.parse()
                    .map_err(|e| format_err!("Failed to parse memory-limit \"{}\":{}", val, e))?;
            }

            if let Some(val) = section.get("zram-fraction") {
                dev.zram_fraction = val.parse()
                    .map_err(|e| format_err!("Failed to parse zram-fraction \"{}\": {}", val, e))?;
            };

            println!("Found configuration for {}: memory-limit={}MB zram-fraction={}",
                     dev.name, dev.memory_limit_mb, dev.zram_fraction);
            self.devices.push(dev);
        }

        Ok(true)
    }
}

fn handle_device(config: &Config, device: &Device, memtotal_mb: f64) -> Result<bool, Error> {
    if memtotal_mb > device.memory_limit_mb as f64 {
        println!("{}: system has too much memory ({:.1}MB), limit is {}MB, ignoring.",
                 device.name,
                 memtotal_mb,
                 device.memory_limit_mb);
        return Ok(false);
    }
    
    let disksize = (device.zram_fraction * memtotal_mb) as u64 * 1024 * 1024;
    let service_name = format!("swap-create@{}.service", device.name);
    println!("Creating {} for /dev/{} ({}MB)",
             service_name, device.name, disksize / 1024 / 1024);
    let device_name = format!("dev-{}.device", device.name);

    let service_path = config.output_directory.join(&service_name);
    let service_path = Path::new(&service_path);
    let mut service = fs::File::create(service_path).with_path(service_path)?;

    let contents = format!("\
[Unit]
Description=Create swap on /dev/%i
Wants=systemd-modules-load.service
After=systemd-modules-load.service
After={device_name}
DefaultDependencies=false

[Service]
Type=oneshot
ExecStart=sh -c 'echo {disksize} >/sys/block/%i/disksize'
ExecStart=mkswap /dev/%i
",
        device_name = device_name,
        disksize = disksize,
    );

    service.write_all(&contents.into_bytes())?;

    let swap_name = format!("dev-{}.swap", device.name);
    let swap_path = config.output_directory.join(&swap_name);
    let swap_path = Path::new(&swap_path);
    let mut swap = fs::File::create(swap_path).with_path(swap_path)?;

    let contents = format!("\
[Unit]
Description=Compressed swap on /dev/{zram_device}
Requires={service}
After={service}

[Swap]
What=/dev/{zram_device}
",
        service = service_name,
        zram_device = device.name
    );

    swap.write_all(&contents.into_bytes())?;

    let symlink_path = config.output_directory.join("swap.target.wants").join(&swap_name);
    let target_path = format!("../{}", swap_name);
    make_symlink(&target_path, &symlink_path)?;
    Ok(true)
}

fn run(config: Config) -> Result<(), Error> {
    let memtotal = get_total_memory()?;
    let memtotal_mb = memtotal as f64 / 1024.;

    let mut some = false;

    if virtualization_container()? {
        println!("Running in a container, exiting.");
        return Ok(());
    }

    for dev in &config.devices {
        let found = handle_device(&config, dev, memtotal_mb)?;
        some |= found;
    }

    if some {
        /* We created some services, let's make sure the module is loaded */
        let modules_load_path = "/run/modules-load.d/zram.conf";
        let modules_load_path = Path::new(&modules_load_path);
        let _ = fs::create_dir(modules_load_path.parent().unwrap());
        let mut modules_load = fs::File::create(modules_load_path).with_path(modules_load_path)?;
        modules_load.write(b"zram\n")?;
    }

    Ok(())
}

fn get_total_memory() -> Result<u64, Error> {
    let path = Path::new("/proc/meminfo");

    let mut file = fs::File::open(&path).with_path(path)?;

    let mut s = String::new();
    file.read_to_string(&mut s)?;

    for line in s.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields[0] != "MemTotal:" {
            continue;
        }

        let memtotal = fields[1].parse::<u64>().unwrap();
        return Ok(memtotal);
    }

    Err(format_err!("Couldn't find MemTotal in {}", path.display()))
}
