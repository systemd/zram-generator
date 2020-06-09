/* SPDX-License-Identifier: MIT */

use anyhow::{anyhow, Context, Result};
use ini::ini::{Ini, Properties as IniProperties, SectionIntoIter};
use std::borrow::Cow;
use std::cmp;
use std::fmt;
use std::fs;
use std::io::{prelude::*, BufReader};
use std::iter::FromIterator;
use std::path::Path;

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
            host_memory_limit_mb: Some(2 * 1024),
            zram_fraction: 0.25,
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
    match read_devices(root)?
        .find(|(section_name, _)| section_name.as_ref().map(String::as_str) == Some(name))
    {
        Some((section_name, section)) => {
            let memtotal_mb = get_total_memory_kb(root)? as f64 / 1024.;
            parse_device(section_name, section, memtotal_mb)
        }
        None => Ok(None),
    }
}

pub fn read_all_devices(root: &Path) -> Result<Vec<Device>> {
    let memtotal_mb = get_total_memory_kb(&root)? as f64 / 1024.;
    Result::from_iter(
        read_devices(root)?
            .map(|(sn, s)| parse_device(sn, s, memtotal_mb))
            .map(Result::transpose)
            .flatten(),
    )
}

fn read_devices(root: &Path) -> Result<SectionIntoIter> {
    let path = root.join("etc/systemd/zram-generator.conf");
    if !path.exists() {
        println!("No configuration file found.");
        return Ok(Ini::new().into_iter());
    }

    Ok(Ini::load_from_file(&path)
        .with_context(|| format!("Failed to read configuration from {}", path.display(),))?
        .into_iter())
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

fn parse_device(
    section_name: Option<String>,
    mut section: IniProperties,
    memtotal_mb: f64,
) -> Result<Option<Device>> {
    let section_name = section_name
        .map(Cow::Owned)
        .unwrap_or(Cow::Borrowed("(no title)"));

    if !section_name.starts_with("zram") {
        println!("Ignoring section \"{}\"", section_name);
        return Ok(None);
    }

    let mut dev = Device::new(section_name.into_owned());

    if let Some(val) = section.get("host-memory-limit") {
        dev.host_memory_limit_mb = parse_optional_size(val)?;
    } else if let Some(val) = section.get("memory-limit") {
        /* For backwards compat. Prefer the new name. */
        dev.host_memory_limit_mb = parse_optional_size(val)?;
    }

    if let Some(val) = section.get("zram-fraction") {
        dev.zram_fraction = val
            .parse()
            .with_context(|| format!("Failed to parse zram-fraction \"{}\"", val))?;
    }

    if let Some(val) = section.get("max-zram-size") {
        dev.max_zram_size_mb = parse_optional_size(val)?;
    }

    if let Some((_, val)) = section.remove_entry("compression-algorithm") {
        dev.compression_algorithm = Some(val);
    }

    println!("Found configuration for {}", dev);

    match dev.host_memory_limit_mb {
        Some(limit) if memtotal_mb > limit as f64 => {
            println!(
                "{}: system has too much memory ({:.1}MB), limit is {}MB, ignoring.",
                dev.name, memtotal_mb, limit,
            );
            Ok(None)
        }
        _ => {
            dev.disksize = (dev.zram_fraction * memtotal_mb) as u64 * 1024 * 1024;
            if let Some(max_mb) = dev.max_zram_size_mb {
                dev.disksize = cmp::min(dev.disksize, max_mb * 1024 * 1024);
            }
            Ok(Some(dev))
        }
    }
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
