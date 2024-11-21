/* SPDX-License-Identifier: MIT */

use anyhow::{anyhow, Context, Result};
use fasteval::Evaler;
use ini::Ini;
use log::{info, warn};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{prelude::*, BufReader};
use std::os::unix::process::ExitStatusExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const DEFAULT_ZRAM_SIZE: &str = "min(ram / 2, 4096)";
const DEFAULT_RESIDENT_LIMIT: &str = "0";

pub struct Device {
    pub name: String,

    pub host_memory_limit_mb: Option<u64>,

    /// Default: `DEFAULT_ZRAM_SIZE`
    pub zram_size: Option<(String, fasteval::ExpressionI, fasteval::Slab)>,
    pub compression_algorithms: Algorithms,
    pub writeback_dev: Option<PathBuf>,
    pub disksize: u64,

    /// /sys/block/zramX/mem_limit; default: `DEFAULT_RESIDENT_LIMIT`
    pub zram_resident_limit: Option<(String, fasteval::ExpressionI, fasteval::Slab)>,
    pub mem_limit: u64,

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
            compression_algorithms: Default::default(),
            writeback_dev: None,
            disksize: 0,
            zram_resident_limit: None,
            mem_limit: 0,
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

    fn process_size(
        &self,
        zram_option: &Option<(String, fasteval::ExpressionI, fasteval::Slab)>,
        ctx: &mut EvalContext,
        default_size: f64,
        label: &str,
    ) -> Result<u64> {
        Ok((match zram_option {
            Some(zs) => {
                zs.1.from(&zs.2.ps)
                    .eval(&zs.2, ctx)
                    .with_context(|| format!("{} {}", self.name, label))
                    .and_then(|f| {
                        if f >= 0. {
                            Ok(f)
                        } else {
                            Err(anyhow!("{}: {}={} < 0", self.name, label, f))
                        }
                    })?
            }
            None => default_size,
        } * 1024.0
            * 1024.0) as u64)
    }

    fn set_disksize_if_enabled(&mut self, ctx: &mut EvalContext) -> Result<()> {
        if !self.is_enabled(ctx.memtotal_mb) {
            return Ok(());
        }

        if self.zram_fraction.is_some() || self.max_zram_size_mb.is_some() {
            // deprecated path
            let max_mb = self.max_zram_size_mb.unwrap_or(None).unwrap_or(u64::MAX);
            self.disksize = ((self.zram_fraction.unwrap_or(0.5) * ctx.memtotal_mb as f64) as u64)
                .min(max_mb)
                * (1024 * 1024);
        } else {
            self.disksize = self.process_size(
                &self.zram_size,
                ctx,
                (ctx.memtotal_mb as f64 / 2.).min(4096.), // DEFAULT_ZRAM_SIZE
                "zram-size",
            )?;
        }

        self.mem_limit = self.process_size(
            &self.zram_resident_limit,
            ctx,
            0., // DEFAULT_RESIDENT_LIMIT
            "zram-resident-limit",
        )?;

        Ok(())
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: host-memory-limit={} zram-size={} zram-resident-limit={} compression-algorithm={} writeback-device={} options={}",
            self.name,
            OptMB(self.host_memory_limit_mb),
            self.zram_size
                .as_ref()
                .map(|zs| &zs.0[..])
                .unwrap_or(DEFAULT_ZRAM_SIZE),
            self.zram_resident_limit
                .as_ref()
                .map(|zs| &zs.0[..])
                .unwrap_or(DEFAULT_RESIDENT_LIMIT),
            self.compression_algorithms,
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

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Algorithms {
    pub compression_algorithms: Vec<(String, String)>, // algorithm, params; first one is real compression, later ones are recompression
    pub recompression_global: String,                  // params
}
impl fmt::Display for Algorithms {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.compression_algorithms[..] {
            [] => f.write_str("<default>")?,
            [(first, firstparams), more @ ..] => {
                f.write_str(first)?;
                if !firstparams.is_empty() {
                    write!(f, " ({})", firstparams)?;
                }
                for (algo, params) in more {
                    write!(f, " then {}", algo)?;
                    if !params.is_empty() {
                        write!(f, " ({})", params)?;
                    }
                }
            }
        }
        if !self.recompression_global.is_empty() {
            write!(f, "(global recompress: {})", self.recompression_global)?;
        }
        Ok(())
    }
}

struct EvalContext {
    memtotal_mb: u64,
    additional: BTreeMap<String, f64>,
}

impl fasteval::EvalNamespace for EvalContext {
    fn lookup(&mut self, name: &str, args: Vec<f64>, _: &mut String) -> Option<f64> {
        if !args.is_empty() {
            None
        } else if name == "ram" {
            Some(self.memtotal_mb as f64)
        } else {
            self.additional.get(name).copied()
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

fn toplevel_line(
    path: &Path,
    k: &str,
    val: &str,
    slab: &mut fasteval::Slab,
    ctx: &mut EvalContext,
) -> Result<()> {
    let (op, arg) = if let Some(colon) = k.find('!') {
        k.split_at(colon + 1)
    } else {
        warn!(
            "{}: invalid outside-of-section key {}, ignoring.",
            path.display(),
            k
        );
        return Ok(());
    };

    match op {
        "set!" => {
            let out = Command::new("/bin/sh")
                .args(["-c", "--", val])
                .stdin(Stdio::null())
                .stderr(Stdio::inherit())
                .output()
                .with_context(|| format!("{}: {}: {}", path.display(), k, val))?;
            let exit = out
                .status
                .code()
                .unwrap_or_else(|| 128 + out.status.signal().unwrap());
            if exit != 0 {
                warn!("{}: {} exited {}", k, val, exit);
            }

            let expr = String::from_utf8(out.stdout)
                .with_context(|| format!("{}: {}: {}", path.display(), k, val))?;
            let evalled = fasteval::Parser::new()
                .parse(&expr, &mut slab.ps)
                .and_then(|p| p.from(&slab.ps).eval(slab, ctx))
                .with_context(|| format!("{}: {}: {}: {}", path.display(), k, val, expr))?;
            ctx.additional.insert(arg.to_string(), evalled);
        }
        _ => warn!(
            "{}: unknown outside-of-section operation {}, ignoring.",
            path.display(),
            op
        ),
    }
    Ok(())
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
    let mut slab = fasteval::Slab::new();
    let mut ctx = EvalContext {
        memtotal_mb,
        additional: BTreeMap::new(),
    };

    for (_, path) in fragments {
        let ini = Ini::load_from_file(&path)?;

        for (sname, props) in ini.iter() {
            let sname = match sname {
                None => {
                    for (k, v) in props.iter() {
                        toplevel_line(&path, k, v, &mut slab, &mut ctx)?;
                    }
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
        dev.set_disksize_if_enabled(&mut ctx)?;
    }

    Ok(devices)
}

fn locate_fragments(root: &Path) -> BTreeMap<OsString, PathBuf> {
    let base_dirs = [
        root.join("usr/lib"),
        root.join("usr/local/lib"),
        root.join("etc"),
        root.join("run"), // We look at /run to allow temporary overriding
                          // of configuration. There is no expectation of
                          // programatic creation of config there.
    ];

    let mut fragments =
        liboverdrop::scan(&base_dirs, "systemd/zram-generator.conf.d", &["conf"], true);

    if let Some(path) = base_dirs
        .into_iter()
        .rev()
        .map(|mut p| {
            p.push("systemd/zram-generator.conf");
            p
        })
        .find(|p| p.exists())
    {
        fragments.insert(OsString::new(), path); // The empty string shall sort earliest
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

fn parse_size_expr(
    dev: &Device,
    key: &str,
    value: &str,
) -> Result<(String, fasteval::ExpressionI, fasteval::Slab)> {
    let mut sl = fasteval::Slab::new();
    Ok((
        value.to_string(),
        fasteval::Parser::new()
            .parse_noclear(value, &mut sl.ps)
            .with_context(|| format!("{} {}", key, dev.name))?,
        sl,
    ))
}

fn parse_compression_algorithm_params(whole: &str) -> (String, String) {
    if let Some(paren) = whole.find('(') {
        let (algo, mut params) = whole.split_at(paren);
        params = &params[1..];
        if params.ends_with(')') {
            params = &params[..params.len() - 1];
        }
        (algo.to_string(), params.replace(',', " "))
    } else {
        (whole.to_string(), String::new())
    }
}

fn parse_line(dev: &mut Device, key: &str, value: &str) -> Result<()> {
    match key {
        "host-memory-limit" | "memory-limit" => {
            /* memory-limit is for backwards compat. host-memory-limit name is preferred. */
            dev.host_memory_limit_mb = parse_optional_size(value)?;
        }

        "zram-size" => {
            dev.zram_size = Some(parse_size_expr(dev, key, value)?);
        }

        "zram-resident-limit" => {
            dev.zram_resident_limit = Some(parse_size_expr(dev, key, value)?);
        }

        "compression-algorithm" => {
            dev.compression_algorithms =
                value
                    .split_whitespace()
                    .fold(Default::default(), |mut algos, s| {
                        let (algo, params) = parse_compression_algorithm_params(s);
                        if algo.is_empty() {
                            algos.recompression_global = params;
                        } else {
                            algos.compression_algorithms.push((algo, params));
                        }
                        algos
                    });
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
        BufReader::new(fs::File::open(path).with_context(|| {
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
        dev.set_disksize_if_enabled(&mut EvalContext {
            memtotal_mb,
            additional: vec![("two".to_string(), 2.)].into_iter().collect(),
        })
        .unwrap();
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
    fn test_eval_size_expression_with_additional() {
        assert_eq!(
            dev_with_zram_size_size(Some("0.5 * ram * two"), 100),
            50 * 2 * 1024 * 1024
        );
    }

    #[test]
    fn test_eval_size_expression_500() {
        assert_eq!(
            dev_with_zram_size_size(Some("500"), 5000),
            500 * 1024 * 1024
        );
    }

    #[test]
    fn test_eval_size_expression_500k() {
        assert_eq!(
            dev_with_zram_size_size(Some("500k"), 5000),
            500 * 1000 * 1024 * 1024
        );
    }

    #[test]
    fn test_eval_size_expression_32g() {
        assert_eq!(
            dev_with_zram_size_size(Some("32G"), 5000),
            32 * 1000_000_000 * 1024 * 1024
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
