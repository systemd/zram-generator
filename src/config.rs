/* SPDX-License-Identifier: MIT */

use anyhow::{anyhow, Context, Result};
use fasteval::Evaler;
use ini::Ini;
use liboverdrop::FragmentScanner;
use log::{info, warn};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::io::{prelude::*, BufReader};
use std::path::{Component, Path, PathBuf};

const DEFAULT_ZRAM_SIZE: &str = "min(ram / 2, 4096)";

pub struct Device {
    pub name: String,

    pub host_memory_limit_mb: Option<u64>,

    /// Default: `DEFAULT_ZRAM_SIZE`
    pub zram_size: Option<(String, fasteval::ExpressionI, fasteval::Slab)>,
    pub compression_algorithm: Option<String>,
    pub writeback_dev: Option<PathBuf>,
    pub disksize: u64,

    pub swap_priority: i32,
    /// when set, a mount unit will be created
    pub mount_point: Option<PathBuf>,
    /// useful mostly for mounts,
    /// None is the same as "swap" when mount_point is not set
    pub fs_type: Option<String>,
    pub options: Cow<'static, str>,

    /// deprecated, overrides zram_size
    pub zram_fraction: Option<f64>,
    /// deprecated, overrides zram_size
    pub max_zram_size_mb: Option<Option<u64>>,
}

impl Device {
    fn new(name: String) -> Device {
        Device {
            name,
            host_memory_limit_mb: None,
            zram_size: None,
            compression_algorithm: None,
            writeback_dev: None,
            disksize: 0,
            swap_priority: 100,
            mount_point: None,
            fs_type: None,
            options: "discard".into(),

            zram_fraction: None,
            max_zram_size_mb: None,
        }
    }

    pub fn is_swap(&self) -> bool {
        self.mount_point.is_none()
            && (self.fs_type.is_none() || self.fs_type.as_ref().unwrap() == "swap")
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
        match (self.fs_type.as_ref(), self.is_swap()) {
            (Some(fs_type), _) => fs_type,
            (None, true) => "swap",
            (None, false) => "ext2",
        }
    }

    fn set_disksize_if_enabled(&mut self, memtotal_mb: u64) -> Result<()> {
        if !self.is_enabled(memtotal_mb) {
            return Ok(());
        }

        if self.zram_fraction.is_some() || self.max_zram_size_mb.is_some() {
            // deprecated path
            let max_mb = self.max_zram_size_mb.unwrap_or(None).unwrap_or(u64::MAX);
            self.disksize = ((self.zram_fraction.unwrap_or(0.5) * memtotal_mb as f64) as u64)
                .min(max_mb)
                * (1024 * 1024);
        } else {
            self.disksize = (match self.zram_size.as_ref() {
                Some(zs) => {
                    zs.1.from(&zs.2.ps)
                        .eval(&zs.2, &mut RamNs(memtotal_mb as f64))
                        .with_context(|| format!("{} zram-size", self.name))
                        .and_then(|f| {
                            if f >= 0. {
                                Ok(f)
                            } else {
                                Err(anyhow!("{}: zram-size={} < 0", self.name, f))
                            }
                        })?
                }
                None => (memtotal_mb as f64 / 2.).min(4096.), // DEFAULT_ZRAM_SIZE
            } * 1024.
                * 1024.) as u64;
        }

        Ok(())
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: host-memory-limit={} zram-size={} compression-algorithm={} writeback-device={} options={}",
            self.name,
            OptMB(self.host_memory_limit_mb),
            self.zram_size
                .as_ref()
                .map(|zs| &zs.0[..])
                .unwrap_or(DEFAULT_ZRAM_SIZE),
            self.compression_algorithm.as_deref().unwrap_or("<default>"),
            self.writeback_dev.as_deref().unwrap_or_else(|| Path::new("<none>")).display(),
            self.options
        )?;
        if self.zram_fraction.is_some() || self.max_zram_size_mb.is_some() {
            f.write_str(" (")?;
            if let Some(zf) = self.zram_fraction {
                write!(f, "zram-fraction={}", zf)?;
            }
            if self.max_zram_size_mb.is_some() {
                f.write_str(" ")?;
            }
            if let Some(mzs) = self.max_zram_size_mb {
                write!(f, "max-zram-size={}", OptMB(mzs))?;
            }
            f.write_str(")")?;
        }
        Ok(())
    }
}

struct OptMB(Option<u64>);
impl fmt::Display for OptMB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(val) => write!(f, "{}MB", val),
            None => f.write_str("<none>"),
        }
    }
}

struct RamNs(f64);
impl fasteval::EvalNamespace for RamNs {
    fn lookup(&mut self, name: &str, args: Vec<f64>, _: &mut String) -> Option<f64> {
        if name == "ram" && args.is_empty() {
            Some(self.0)
        } else {
            None
        }
    }
}

pub fn read_device(root: &Path, kernel_override: bool, name: &str) -> Result<Option<Device>> {
    let memtotal_mb = get_total_memory_kb(root)? as f64 / 1024.;
    Ok(read_devices(root, kernel_override, memtotal_mb as u64)?
        .remove(name)
        .filter(|dev| dev.disksize > 0))
}

pub fn read_all_devices(root: &Path, kernel_override: bool) -> Result<Vec<Device>> {
    let memtotal_mb = get_total_memory_kb(root)? as f64 / 1024.;
    Ok(read_devices(root, kernel_override, memtotal_mb as u64)?
        .into_iter()
        .filter(|(_, dev)| dev.disksize > 0)
        .map(|(_, dev)| dev)
        .collect())
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
        dev.set_disksize_if_enabled(memtotal_mb)?;
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
    if let Some(path) = base_dirs
        .into_iter()
        .rev()
        .map(PathBuf::from)
        .map(|mut p| {
            p.push("systemd/zram-generator.conf");
            p
        })
        .find(|p| p.exists())
    {
        fragments.insert(String::new(), path); // The empty string shall sort earliest
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
        -1..=0x7FFF => Ok(val),
        _ => Err(anyhow!("Swap priority {} out of range", val)),
    }
}

fn verify_mount_point(key: &str, val: &str) -> Result<PathBuf> {
    let path = Path::new(val);

    if path.is_relative() {
        return Err(anyhow!("{} {} is not absolute", key, val));
    }

    if path.components().any(|c| c == Component::ParentDir) {
        return Err(anyhow!("{} {:#?} is not normalized", key, path));
    }

    Ok(path.components().collect()) // normalise away /./ components
}

fn parse_line(dev: &mut Device, key: &str, value: &str) -> Result<()> {
    match key {
        "host-memory-limit" | "memory-limit" => {
            /* memory-limit is for backwards compat. host-memory-limit name is preferred. */
            dev.host_memory_limit_mb = parse_optional_size(value)?;
        }

        "zram-size" => {
            let mut sl = fasteval::Slab::new();
            dev.zram_size = Some((
                value.to_string(),
                fasteval::Parser::new()
                    .parse_noclear(value, &mut sl.ps)
                    .with_context(|| format!("{} zram-size", dev.name))?,
                sl,
            ));
        }

        "compression-algorithm" => {
            dev.compression_algorithm = Some(value.to_string());
        }

        "writeback-device" => {
            dev.writeback_dev = Some(verify_mount_point(key, value)?);
        }

        "swap-priority" => {
            dev.swap_priority = parse_swap_priority(value)?;
        }

        "mount-point" => {
            dev.mount_point = Some(verify_mount_point(key, value)?);
        }

        "fs-type" => {
            dev.fs_type = Some(value.to_string());
        }

        "options" => {
            dev.options = value.to_string().into();
        }

        "zram-fraction" => {
            /* zram-fraction is for backwards compat. zram-size = is preferred. */

            dev.zram_fraction = Some(
                value
                    .parse()
                    .with_context(|| format!("Failed to parse zram-fraction \"{}\"", value))
                    .and_then(|f| {
                        if f >= 0. {
                            Ok(f)
                        } else {
                            Err(anyhow!("{}: zram-fraction={} < 0", dev.name, f))
                        }
                    })?,
            );
        }

        "max-zram-size" => {
            /* zram-fraction is for backwards compat. zram-size = is preferred. */

            dev.max_zram_size_mb = Some(parse_optional_size(value)?);
        }

        _ => {
            warn!("{}: unknown key {}, ignoring.", dev.name, key);
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
        if let (Some("MemTotal:"), Some(val)) = (fields.next(), fields.next()) {
            return Ok(val.parse()?);
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

    // Last argument wins
    Ok(text
        .split_whitespace()
        .rev()
        .filter(|w| w.starts_with(word))
        .flat_map(|w| match &w[word.len()..] {
            "" | "=1" | "=yes" | "=true" | "=on" => Some(true),
            "=0" | "=no" | "=false" | "=off" => Some(false),
            _ => None,
        })
        .next())
}

pub fn kernel_has_option(root: &Path, word: &str) -> Result<Option<bool>> {
    let path = root.join("proc/cmdline");
    _kernel_has_option(&path, word)
}

pub fn kernel_zram_option(root: &Path) -> Option<bool> {
    match kernel_has_option(root, "systemd.zram") {
        Ok(r @ Some(true)) | Ok(r @ None) => r,
        Ok(Some(false)) => {
            info!("Disabled by systemd.zram option in /proc/cmdline.");
            Some(false)
        }
        Err(e) => {
            warn!("Failed to parse /proc/cmdline ({}), ignoring.", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with(data: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write(data).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_get_total_memory_kb() {
        let file = file_with(
            b"\
MemTotal:        8013220 kB
MemFree:          721288 kB
MemAvailable:    1740336 kB
Buffers:          292752 kB
",
        );
        let mem = _get_total_memory_kb(file.path()).unwrap();
        assert_eq!(mem, 8013220);
    }

    #[test]
    #[should_panic(expected = "Couldn't find MemTotal")]
    fn test_get_total_memory_not_found() {
        let file = file_with(
            b"\
MemTotala:        8013220 kB
aMemTotal:        8013220 kB
MemTotal::        8013220 kB
",
        );
        _get_total_memory_kb(file.path()).unwrap();
    }

    #[test]
    fn test_kernel_has_option() {
        let file = file_with(b"foo=1 foo=0 foo=on foo=off foo\n");
        assert_eq!(_kernel_has_option(file.path(), "foo").unwrap(), Some(true));
    }

    #[test]
    fn test_kernel_has_no_option() {
        let file = file_with(
            b"\
foo=1
foo=0
",
        );
        assert_eq!(_kernel_has_option(file.path(), "foo").unwrap(), Some(false));
    }

    #[test]
    fn test_verify_mount_point() {
        for e in ["foo/bar", "/foo/../bar", "/foo/.."] {
            assert!(verify_mount_point("test", e).is_err(), "{}", e);
        }

        for (p, o) in [
            ("/foobar", "/foobar"),
            ("/", "/"),
            ("//", "/"),
            ("///", "/"),
            ("/foo/./bar/", "/foo/bar"),
        ] {
            assert_eq!(
                verify_mount_point("test", p).unwrap(),
                Path::new(o),
                "{} vs {}",
                p,
                o
            );
        }
    }

    fn dev_with_zram_size_size(val: Option<&str>, memtotal_mb: u64) -> u64 {
        let mut dev = Device::new("zram0".to_string());
        if let Some(val) = val {
            parse_line(&mut dev, "zram-size", val).unwrap();
        }
        assert!(dev.is_enabled(memtotal_mb));
        dev.set_disksize_if_enabled(memtotal_mb).unwrap();
        dev.disksize
    }

    #[test]
    fn test_eval_size_expression() {
        assert_eq!(
            dev_with_zram_size_size(Some("0.5 * ram"), 100),
            50 * 1024 * 1024
        );
    }

    #[test]
    fn test_eval_size_expression_default() {
        assert_eq!(dev_with_zram_size_size(None, 100), 50 * 1024 * 1024);
        assert_eq!(dev_with_zram_size_size(None, 10000), 4096 * 1024 * 1024);
    }

    #[test]
    fn test_eval_size_expression_default_equivalent() {
        assert_eq!(
            dev_with_zram_size_size(Some(DEFAULT_ZRAM_SIZE), 100),
            50 * 1024 * 1024
        );
        assert_eq!(
            dev_with_zram_size_size(Some(DEFAULT_ZRAM_SIZE), 10000),
            4096 * 1024 * 1024
        );
    }

    #[test]
    #[should_panic(expected = "Undefined(\"array\")")]
    fn test_eval_size_expression_unknown_variable() {
        dev_with_zram_size_size(Some("array(1,2)"), 100);
    }

    #[test]
    #[should_panic(expected = "zram-size=NaN")]
    fn test_eval_size_expression_nan() {
        dev_with_zram_size_size(Some("(ram-100)/0"), 100);
    }

    #[test]
    fn test_eval_size_expression_inf() {
        assert_eq!(dev_with_zram_size_size(Some("(ram-99)/0"), 100), u64::MAX); // +∞
    }

    #[test]
    fn test_eval_size_expression_min() {
        assert_eq!(
            dev_with_zram_size_size(Some("min(0.5 * ram, 4000)"), 3000),
            1500 * 1024 * 1024
        );
    }
}
