/* SPDX-License-Identifier: MIT */

#[macro_use]
extern crate failure;
extern crate ini;

use failure::Error;
use ini::Ini;
use std::fs;
use std::os::unix::fs::symlink;
use std::result;
use std::fmt;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::env;

pub trait ResultExt<T, E>: failure::ResultExt<T, E> where E: fmt::Display {
    fn with_path<P: AsRef<Path>>(
        self,
        path: P,
    ) -> result::Result<T, failure::Context<String>>
    where Self: Sized
    {
        self.with_context(|e| format!("{}: {}", path.as_ref().display(), e))
    }
}

impl<T, E: fmt::Display>
    ResultExt<T, E> for result::Result<T, E>
where result::Result<T, E>: failure::ResultExt<T, E> {}


fn make_symlink(dst: &str, src: &PathBuf) -> Result<(), Error> {
    let parent = src.parent().unwrap();

    let _ = fs::create_dir(&parent);
    symlink(dst, src).with_path(src)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let config = match Config::new(&args) {
        Ok(ok) => ok,
        Err(_e) => std::process::exit(1),
    };

    if let Err(e) = run(config) {
        println!("{}", e);
        std::process::exit(2);
    }
}

struct Config {
    output_directory: PathBuf,
    zram_device: String,
    memory_limit_mb: u64,
    zram_fraction: f64,
}

impl Config {
    fn new(args: &[String]) -> Result<Config, Error> {
        let output_directory;

        match args.len() {
            2 | 4 => { output_directory = PathBuf::from(&args[1]) },
            _ => return Err(failure::err_msg("This program requires 1 or 3 arguments")),
        }

        let zram_device = String::from("zram0");
        let memory_limit_mb = 2*1024;
        let zram_fraction = 0.25;

        let mut config = Config { output_directory, zram_device, memory_limit_mb, zram_fraction };
        config.read();
        Ok(config)
    }

    fn read(&mut self) {
        let path = Path::new("/etc/systemd/zram-generator.conf");
        if !path.exists() {
            return;
        }

        let conf = Ini::load_from_file(path).with_path(path).unwrap();

        if let Some(section) = conf.section(Some("zram0".to_owned())) {
            if let Some(mem) = section.get("memory-limit") {
                self.memory_limit_mb = mem.parse().unwrap();
            };

            if let Some(fra) = section.get("zram-fraction") {
                self.zram_fraction = fra.parse().unwrap();
            };
        }
    }
}

fn run(config: Config) -> Result<(), Error> {
    let memtotal = get_total_memory()?;

    if memtotal as f64 / 1024. > config.memory_limit_mb as f64 {
        println!("System has too much memory ({:.1}MB), limit is {}MB, exiting.",
                 memtotal as f64 / 1024.,
                 config.memory_limit_mb);
        return Ok(());
    }

    let disksize = (config.zram_fraction * memtotal as f64) as u64 * 1024;
    let service_name = format!("swap-create@{}.service", config.zram_device);
    println!("Creating {} for /dev/{} ({}MB)", service_name, config.zram_device, disksize/1024/1024);

    let _service_path = config.output_directory.join(&service_name);
    let service_path = Path::new(&_service_path);
    let mut service = fs::File::create(service_path).with_path(service_path)?;

    let contents = format!("\
        [Unit]
Description=Create swap on /dev/%i
Wants=systemd-modules-load.service
After=systemd-modules-load.service

[Service]
Type=oneshot
ExecStart=sh -c 'echo {disksize} >/sys/block/%i/disksize'
ExecStart=mkswap /dev/%i
", disksize=disksize);

    service.write_all(&contents.into_bytes())?;

    let _swap_path = config.output_directory.join("dev-zram0.swap");
    let swap_path = Path::new(&_swap_path);
    let mut swap = fs::File::create(swap_path).with_path(swap_path)?;

    let contents = format!("\
        [Unit]
Description=Compressed swap on /dev/{zram_device}
Requires={service}
After={service}

[Swap]
What=/dev/{zram_device}
", service=service_name, zram_device=config.zram_device);

    swap.write_all(&contents.into_bytes())?;

    let wants_dir = config.output_directory.join("swap.target.wants");
    let symlink_path = wants_dir.join("dev-zram0.swap");
    make_symlink("../dev-zram0.swap", &symlink_path)?;

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
