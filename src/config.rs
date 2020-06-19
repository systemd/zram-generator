/* SPDX-License-Identifier: MIT */

use anyhow::{anyhow, Context, Result};
use ini::Ini;
use liboverdrop::FragmentScanner;
use std::cmp;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::io::{prelude::*, BufReader};
use std::path::{Path, PathBuf};

pub struct Device {
    pub name: String,
    pub host_memory_limit_mb: Option<u64>,
    pub zram_fraction: f64,
    pub max_zram_size_mb: Option<u64>,
    pub compression_algorithm: Option<String>,
    pub disksize: u64,
}

impl Device {
    fn new(name: String) -> Device {
        Device {
            name,
            host_memory_limit_mb: None,
            zram_fraction: 0.5,
            max_zram_size_mb: Some(4 * 1024),
            compression_algorithm: None,
            disksize: 0,
        }
    }

    fn write_optional_mb(f: &mut fmt::Formatter<'_>, val: Option<u64>) -> fmt::Result {
        match val {
            Some(val) => {
                write!(f, "{}", val)?;
                f.write_str("MB")?;
            }
            None => f.write_str("<none>")?,
        }
        Ok(())
    }

    fn is_enabled(&self, memtotal_mb: u64) -> bool {
        match self.host_memory_limit_mb {
            Some(limit_mb) if limit_mb < memtotal_mb => {
                println!(
                    "{}: system has too much memory ({:.1}MB), limit is {}MB, ignoring.",
                    self.name,
                    memtotal_mb,
                    self.host_memory_limit_mb.unwrap()
                );

                false
            }
            _ => true,
        }
    }

    fn set_disksize_if_enabled(&mut self, memtotal_mb: u64) {
        if !self.is_enabled(memtotal_mb) {
            return;
        }

        self.disksize = (self.zram_fraction * memtotal_mb as f64) as u64 * 1024 * 1024;
        if let Some(max_mb) = self.max_zram_size_mb {
            self.disksize = cmp::min(self.disksize, max_mb * 1024 * 1024);
        }
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: host-memory-limit=", self.name)?;
        Device::write_optional_mb(f, self.host_memory_limit_mb)?;
        write!(f, " zram-fraction={} max-zram-size=", self.zram_fraction)?;
        Device::write_optional_mb(f, self.max_zram_size_mb)?;
        f.write_str(" compression-algorithm=")?;
        match self.compression_algorithm.as_ref() {
            Some(alg) => f.write_str(alg)?,
            None => f.write_str("<default>")?,
        }
        Ok(())
    }
}

pub fn read_device(root: &Path, name: &str) -> Result<Option<Device>> {
    let memtotal_mb = (get_total_memory_kb(&root)? as f64 / 1024.) as u64;
    Ok(read_devices(root, memtotal_mb)?.remove(name))
}

pub fn read_all_devices(root: &Path) -> Result<Vec<Device>> {
    let memtotal_mb = get_total_memory_kb(&root)? / 1024;

    let devices: Vec<Device> = read_devices(root, memtotal_mb)?
        .into_iter()
        .filter(|(_, dev)| dev.disksize > 0)
        .map(|(_, dev)| dev)
        .collect();

    Ok(devices)
}

fn read_devices(root: &Path, memtotal_mb: u64) -> Result<HashMap<String, Device>> {
    let fragments = locate_fragments(root);

    if fragments.is_empty() {
        println!("No configuration file found.");
    }

    let mut devices: HashMap<String, Device> = HashMap::new();

    for (_, path) in fragments {
        let ini = Ini::load_from_file(&path)?;

        for (sname, props) in ini.iter() {
            let sname = match sname {
                None => {
                    eprintln!(
                        "{:?}: ignoring settings outside of section: {:?}",
                        path, props
                    );
                    continue;
                }
                Some(sname) if sname.starts_with("zram") && sname[4..].parse::<u64>().is_ok() => {
                    sname.to_string()
                }
                Some(sname) => {
                    println!("Ignoring section \"{}\"", sname);
                    continue;
                }
            };

            let dev = devices
                .entry(sname.clone())
                .or_insert_with(|| Device::new(sname));

            for (k, v) in props.iter() {
                parse_line(dev, k, v)?;
            }
        }
    }

    for dev in devices.values_mut() {
        dev.set_disksize_if_enabled(memtotal_mb);
    }

    Ok(devices)
}

fn locate_fragments(root: &Path) -> BTreeMap<String, PathBuf> {
    let base_dirs = vec![
        String::from(root.join("usr/lib").to_str().unwrap()),
        String::from(root.join("usr/local/lib").to_str().unwrap()),
        String::from(root.join("etc").to_str().unwrap()),
        String::from(root.join("run").to_str().unwrap()), // We look at /run to allow temporary overriding
                                                          // of configuration. There is no expectation of
                                                          // programatic creation of config there.
    ];

    let cfg = FragmentScanner::new(
        base_dirs.clone(),
        "systemd/zram-generator.conf.d",
        true,
        vec![String::from("conf")],
    );

    let mut fragments = cfg.scan();

    for dir in base_dirs.iter().rev() {
        let path = PathBuf::from(dir).join("systemd/zram-generator.conf");
        if path.exists() {
            fragments.insert("".to_string(), path); // The empty string shall sort earliest
            break;
        }
    }

    fragments
}

fn parse_optional_size(val: &str) -> Result<Option<u64>> {
    Ok(if val == "none" {
        None
    } else {
        Some(
            val.parse()
                .with_context(|| format!("Failed to parse optional size \"{}\"", val))?,
        )
    })
}

fn parse_line(dev: &mut Device, key: &str, value: &str) -> Result<()> {
    match key {
        "host-memory-limit" | "memory-limit" => {
            /* memory-limit is for backwards compat. host-memory-limit name is preferred. */
            dev.host_memory_limit_mb = parse_optional_size(value)?;
        }

        "zram-fraction" => {
            dev.zram_fraction = value
                .parse()
                .with_context(|| format!("Failed to parse zram-fraction \"{}\"", value))
                .and_then(|f| {
                    if f >= 0. {
                        Ok(f)
                    } else {
                        Err(anyhow!("zram-fraction {} < 0", f))
                    }
                })?;
        }

        "max-zram-size" => {
            dev.max_zram_size_mb = parse_optional_size(value)?;
        }

        "compression-algorithm" => {
            dev.compression_algorithm = Some(value.to_string());
        }

        _ => {
            eprintln!("Unknown key {}, ignoring.", key);
        }
    }

    Ok(())
}

fn _get_total_memory_kb(path: &Path) -> Result<u64> {
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

fn get_total_memory_kb(root: &Path) -> Result<u64> {
    let path = root.join("proc/meminfo");
    _get_total_memory_kb(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_total_memory_kb() {
        let mut file = tempfile::NamedTempFile::new().unwrap();

        file.write(
            b"\
MemTotal:        8013220 kB
MemFree:          721288 kB
MemAvailable:    1740336 kB
Buffers:          292752 kB
",
        )
        .unwrap();
        file.flush().unwrap();
        let mem = _get_total_memory_kb(file.path()).unwrap();
        assert_eq!(mem, 8013220);
    }

    #[test]
    #[should_panic(expected = "Couldn't find MemTotal")]
    fn test_get_total_memory_not_found() {
        let mut file = tempfile::NamedTempFile::new().unwrap();

        file.write(
            b"\
MemTotala:        8013220 kB
aMemTotal:        8013220 kB
MemTotal::        8013220 kB
",
        )
        .unwrap();
        file.flush().unwrap();
        _get_total_memory_kb(file.path()).unwrap();
    }
}
