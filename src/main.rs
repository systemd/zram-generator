/* SPDX-License-Identifier: MIT */

use anyhow::{anyhow, Context, Result};
use ini::Ini;
use std::borrow::Cow;
use std::env;
use std::fs;
use std::io::{prelude::*, BufReader};
use std::iter::FromIterator;
use std::os::unix::fs::symlink;
use std::path::{self, Path, PathBuf};
use std::process::{Command, Stdio};

fn make_parent(of: &Path) -> Result<()> {
    let parent = of
        .parent()
        .ok_or_else(|| anyhow!("Couldn't get parent of {}", of.display()))?;
    fs::create_dir_all(&parent)?;
    Ok(())
}

fn make_symlink(dst: &str, src: &Path) -> Result<()> {
    make_parent(src)?;
    symlink(dst, src).with_context(|| {
        format!(
            "Failed to create symlink at {} (pointing to {})",
            dst,
            src.display()
        )
    })?;
    Ok(())
}

fn virtualization_container() -> Result<bool> {
    match Command::new("systemd-detect-virt").arg("--container").stdout(Stdio::null()).status() {
        Ok(status) => Ok(status.success()),
        Err(e) => Err(anyhow!("systemd-detect-virt call failed: {}", e)),
    }
}

fn main() -> Result<()> {
    let root: Cow<'static, str> = env::var("ZRAM_GENERATOR_ROOT")
        .map(|mut root| {
            if !root.ends_with(path::is_separator) {
                root.push('/');
            }
            println!("Using {:?} as root directory", root);
            root.into()
        }).unwrap_or_else(|_| "/".into());

    let args: Vec<String> = env::args().collect();
    let config = Config::new(&args, root)?;

    if config.devices.is_empty() {
        println!("No devices configured, exiting.");
        return Ok(());
    }

    run(config)
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
    root: Cow<'static, str>,
    output_directory: PathBuf,
    devices: Vec<Device>,
}

impl Config {
    fn new(args: &[String], root: Cow<'static, str>) -> Result<Config> {
        let output_directory = match args.len() {
            2 | 4 => PathBuf::from(&args[1]),
            _ => return Err(anyhow!("This program requires 1 or 3 arguments")),
        };

        let devices = Config::read_devices(&root)?;
        Ok(Config { root, output_directory, devices })
    }

    fn read_devices(root: &str) -> Result<Vec<Device>> {
        let path = Path::new(root).join("etc/systemd/zram-generator.conf");
        if !path.exists() {
            println!("No configuration file found.");
            return Ok(vec![]);
        }

        Result::from_iter(Ini::load_from_file(&path).with_context(|| format!("Failed to read configuration from {}", path.display()))?.into_iter().map(|(section_name, section)| {
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

            println!("Found configuration for {}: memory-limit={}MB zram-fraction={}",
                     dev.name, dev.memory_limit_mb, dev.zram_fraction);

            Ok(Some(dev))
        }).map(Result::transpose).flatten())
    }
}

fn handle_device(config: &Config, device: &Device, memtotal_mb: f64) -> Result<bool> {
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

    let service_path = config.output_directory.join(&service_name);

    let contents = format!("\
[Unit]
Description=Create swap on /dev/%i
Wants=systemd-modules-load.service
After=systemd-modules-load.service
After={device_name}
DefaultDependencies=false

[Service]
Type=oneshot
ExecStartPre=-modprobe zram
ExecStart=sh -c 'echo {disksize} >/sys/block/%i/disksize'
ExecStart=mkswap /dev/%i
",
        device_name = format!("dev-{}.device", device.name),
        disksize = disksize,
    );
    fs::write(&service_path, contents).with_context(|| {
        format!(
            "Failed to write a device service into {}",
            service_path.display()
        )
    })?;

    let swap_name = format!("dev-{}.swap", device.name);
    let swap_path = config.output_directory.join(&swap_name);

    let contents = format!("\
[Unit]
Description=Compressed swap on /dev/{zram_device}
Requires={service}
After={service}

[Swap]
What=/dev/{zram_device}
Options=pri=100
",
        service = service_name,
        zram_device = device.name
    );

    fs::write(&swap_path, contents).with_context(|| {
        format!(
            "Failed to write a swap service into {}",
            swap_path.display()
        )
    })?;

    let symlink_path = config.output_directory.join("swap.target.wants").join(&swap_name);
    let target_path = format!("../{}", swap_name);
    make_symlink(&target_path, &symlink_path)?;
    Ok(true)
}

fn run(config: Config) -> Result<()> {
    let memtotal = get_total_memory_kb(&config.root)?;
    let memtotal_mb = memtotal as f64 / 1024.;

    if virtualization_container()? {
        println!("Running in a container, exiting.");
        return Ok(());
    }

    let mut devices_made = false;
    for dev in &config.devices {
        devices_made |= handle_device(&config, dev, memtotal_mb)?;
    }
    if devices_made {
        /* We created some services, let's make sure the module is loaded */
        let modules_load_path = Path::new(&config.root[..]).join("run/modules-load.d/zram.conf");
        make_parent(&modules_load_path)?;
        fs::write(&modules_load_path, "zram\n").with_context(|| {
            format!(
                "Failed to write configuration for loading a module at {}",
                modules_load_path.display()
            )
        })?;
    }

    Ok(())
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
