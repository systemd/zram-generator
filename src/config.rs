/* SPDX-License-Identifier: MIT */

use anyhow::{anyhow, Context, Result};
use ini::Ini;
use liboverdrop::FragmentScanner;
use log::{info, warn};
use std::cmp;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::io::{prelude::*, BufReader};
use std::path::{Component, Path, PathBuf};

pub struct Device {
    pub name: String,

    pub host_memory_limit_mb: Option<u64>,

    pub zram_fraction: f64,
    pub max_zram_size_mb: Option<u64>,
    pub compression_algorithm: Option<String>,
    pub disksize: u64,

    pub swap_priority: i32,
    pub mount_point: Option<PathBuf>, // when set, a mount unit will be created
    pub fs_type: Option<String>,      // useful mostly for mounts, None is the same
                                      // as "swap" when mount_point is not set
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
            swap_priority: 100,
            mount_point: None,
            fs_type: None,
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

    pub fn is_swap(&self) -> bool {
        return self.mount_point.is_none()
            && (self.fs_type.is_none() || self.fs_type.as_ref().unwrap() == "swap");
    }

    fn is_enabled(&self, memtotal_mb: u64) -> bool {
        match self.host_memory_limit_mb {
            Some(limit_mb) if limit_mb < memtotal_mb => {
                info!(
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

    pub fn effective_fs_type(&self) -> &str {
        match self.fs_type {
            Some(ref fs_type) => fs_type,
            None => match self.is_swap() {
                true => "swap",
                false => "ext2",
            },
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

pub fn read_device(root: &Path, kernel_override: bool, name: &str) -> Result<Option<Device>> {
    let memtotal_mb = (get_total_memory_kb(&root)? as f64 / 1024.) as u64;
    Ok(read_devices(root, kernel_override, memtotal_mb)?
        .remove(name)
        .filter(|dev| dev.disksize > 0))
}

pub fn read_all_devices(root: &Path, kernel_override: bool) -> Result<Vec<Device>> {
    let memtotal_mb = get_total_memory_kb(&root)? / 1024;

    let devices: Vec<Device> = read_devices(root, kernel_override, memtotal_mb)?
        .into_iter()
        .filter(|(_, dev)| dev.disksize > 0)
        .map(|(_, dev)| dev)
        .collect();

    Ok(devices)
}

fn read_devices(
    root: &Path,
    kernel_override: bool,
    memtotal_mb: u64,
) -> Result<HashMap<String, Device>> {
    let fragments = locate_fragments(root);

    if fragments.is_empty() && !kernel_override {
        info!("No configuration found.");
    }

    let mut devices: HashMap<String, Device> = HashMap::new();

    for (_, path) in fragments {
        let ini = Ini::load_from_file(&path)?;

        for (sname, props) in ini.iter() {
            let sname = match sname {
                None => {
                    warn!(
                        "{}: ignoring settings outside of section: {:?}",
                        path.display(),
                        props
                    );
                    continue;
                }
                Some(sname) if sname.starts_with("zram") && sname[4..].parse::<u64>().is_ok() => {
                    sname.to_string()
                }
                Some(sname) => {
                    warn!("{}: Ignoring section \"{}\"", path.display(), sname);
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

    if kernel_override {
        devices
            .entry("zram0".to_string())
            .or_insert_with(|| Device::new("zram0".to_string()));
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

fn parse_swap_priority(val: &str) -> Result<i32> {
    let val = val
        .parse()
        .with_context(|| format!("Failed to parse priority \"{}\"", val))?;

    /* See --priority in swapon(8). */
    match val {
        -1..=32767 => Ok(val),
        _ => Err(anyhow!("Swap priority {} out of range", val)),
    }
}

fn verify_mount_point(val: &str) -> Result<PathBuf> {
    let path = PathBuf::from(val);

    if path.is_relative() {
        return Err(anyhow!("mount-point {} is not absolute", val));
    }

    if path.components().any(|c| c == Component::ParentDir) {
        return Err(anyhow!("mount-point {:#?} is not normalized", path));
    }

    Ok(path)
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

        "swap-priority" => {
            dev.swap_priority = parse_swap_priority(value)?;
        }

        "mount-point" => {
            dev.mount_point = Some(verify_mount_point(value)?);
        }

        "fs-type" => {
            dev.fs_type = Some(value.to_string());
        }

        _ => {
            warn!("Unknown key {}, ignoring.", key);
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

fn _kernel_has_option(path: &Path, word: &str) -> Result<Option<bool>> {
    let text = fs::read_to_string(path)?;

    // The last argument wins, so check all words in turn.
    Ok(text.split_whitespace().fold(None, |acc, w| {
        if !w.starts_with(word) {
            acc
        } else {
            match &w[word.len()..] {
                "" | "=1" | "=yes" | "=true" | "=on" => Some(true),
                "=0" | "=no" | "=false" | "=off" => Some(false),
                _ => acc,
            }
        }
    }))
}

pub fn kernel_has_option(root: &Path, word: &str) -> Result<Option<bool>> {
    let path = root.join("proc/cmdline");
    _kernel_has_option(&path, word)
}

pub fn kernel_zram_option(root: &Path) -> Option<bool> {
    match kernel_has_option(root, "systemd.zram") {
        Ok(Some(true)) => Some(true),
        Ok(Some(false)) => {
            info!("Disabled by systemd.zram option in /proc/cmdline.");
            Some(false)
        }
        Ok(None) => None,
        Err(e) => {
            warn!("Failed to parse /proc/cmdline ({}), ignoring.", e);
            None
        }
    }
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

    #[test]
    fn test_kernel_has_option() {
        let mut file = tempfile::NamedTempFile::new().unwrap();

        file.write(
            b"\
foo=1 foo=0 foo=on foo=off foo
",
        )
        .unwrap();
        file.flush().unwrap();
        let foo = _kernel_has_option(file.path(), "foo").unwrap();
        assert_eq!(foo, Some(true));
    }

    #[test]
    fn test_kernel_has_no_option() {
        let mut file = tempfile::NamedTempFile::new().unwrap();

        file.write(
            b"\
foo=1
foo=0
",
        )
        .unwrap();
        file.flush().unwrap();
        let foo = _kernel_has_option(file.path(), "foo").unwrap();
        assert_eq!(foo, Some(false));
    }

    #[test]
    fn test_verify_mount_point() {
        let p = verify_mount_point("/foobar").unwrap();
        assert_eq!(p, PathBuf::from("/foobar"));
    }

    #[test]
    fn test_verify_mount_point_absolute() {
        let p = verify_mount_point("foo/bar");
        assert!(p.is_err());
    }

    #[test]
    fn test_verify_mount_point_normalized() {
        let p = verify_mount_point("/foo/../bar");
        assert!(p.is_err());
    }

    #[test]
    fn test_verify_mount_point_normalized2() {
        let p = verify_mount_point("/foo/..");
        assert!(p.is_err());
    }

    #[test]
    fn test_verify_mount_point_self() {
        verify_mount_point("/foo/./bar/").unwrap();
    }
}
