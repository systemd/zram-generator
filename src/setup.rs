/* SPDX-License-Identifier: MIT */

use crate::config::Device;
use anyhow::{anyhow, Context, Result};
use log::warn;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Command;

pub const SYSTEMD_MAKEFS_COMMAND: &str = concat!(
    env!(
        "SYSTEMD_UTIL_DIR",
        "Define $SYSTEMD_UTIL_DIR to the result of \
         $(pkg-config --variable=systemdutildir systemd) (e.g. /usr/lib/systemd/)"
    ),
    "/systemd-makefs"
);
/// A constant string for use in clap --help output.
#[rustfmt::skip]
pub const AFTER_HELP: &str = concat!(
    "Uses ", env!("SYSTEMD_UTIL_DIR"), "/systemd-makefs", "."
);

pub fn run_device_setup(device: Option<Device>, device_name: &str) -> Result<()> {
    let device = device.ok_or_else(|| anyhow!("Device {} not found", device_name))?;

    let device_sysfs_path = Path::new("/sys/block").join(device_name);

    for (prio, (algo, params)) in device
        .compression_algorithms
        .compression_algorithms
        .iter()
        .enumerate()
    {
        let params = if params.is_empty() {
            None
        } else {
            Some(params)
        };
        let (path, data, add_pathdata) = if prio == 0 {
            (
                device_sysfs_path.join("comp_algorithm"),
                algo,
                params.as_ref().map(|p| {
                    (
                        device_sysfs_path.join("algorithm_params"),
                        format!("algo={algo} {p}"),
                    )
                }),
            )
        } else {
            (
                device_sysfs_path.join("recomp_algorithm"),
                &format!("algo={algo} priority={prio}"),
                params.as_ref().map(|p| {
                    (
                        device_sysfs_path.join("recompress"),
                        format!("{p} priority={prio}"),
                    )
                }),
            )
        };

        match fs::write(&path, data) {
            Ok(_) => {
                if let Some((add_path, add_data)) = add_pathdata {
                    match fs::write(add_path, add_data) {
                        Ok(_) => {}
                        Err(err) => {
                            warn!(
                                "Warning: algorithm {algo:?} supplemental data {data:?} not written: {err}",
                            );
                        }
                    }
                }
            }
            Err(err) if err.kind() == ErrorKind::InvalidInput => {
                warn!(
                    "Warning: algorithm {:?} not recognised; consult {} for a list of available ones",
                    algo, path.display(),
                );
            }
            Err(err) if err.kind() == ErrorKind::PermissionDenied && prio != 0 => {
                warn!(
                    "Warning: recompression algorithm {:?} requested but recompression not available ({} doesn't exist)",
                    algo, path.display(),
                );
            }
            err @ Err(_) => err.with_context(|| {
                format!(
                    "Failed to configure compression algorithm into {}",
                    path.display()
                )
            })?,
        }
    }

    if let Some(ref wb_dev) = device.writeback_dev {
        let writeback_path = device_sysfs_path.join("backing_dev");
        if writeback_path.exists() {
            fs::write(&writeback_path, wb_dev.as_os_str().as_bytes()).with_context(|| {
                format!(
                    "Failed to configure write-back device into {}",
                    writeback_path.display()
                )
            })?;
        } else {
            warn!("Warning: writeback-device={} set for {}, but system doesn't support write-back. Ignoring.", writeback_path.display(), device_name)
        }
    }

    let resident_memory = device_sysfs_path.join("mem_limit");
    fs::write(&resident_memory, format!("{}", device.mem_limit)).with_context(|| {
        format!(
            "Failed to configure resident memory limit into {}",
            resident_memory.display()
        )
    })?;

    let disksize_path = device_sysfs_path.join("disksize");
    fs::write(&disksize_path, format!("{}", device.disksize)).with_context(|| {
        format!(
            "Failed to configure disk size into {}",
            disksize_path.display()
        )
    })?;

    let fs_type = device.effective_fs_type();
    match Command::new(SYSTEMD_MAKEFS_COMMAND).arg(fs_type).arg(Path::new("/dev").join(device_name)).status() {
        Ok(status) =>
            match status.code() {
                Some(0) => Ok(()),
                Some(code) => Err(anyhow!("{} failed with exit code {}", SYSTEMD_MAKEFS_COMMAND, code)),
                None => Err(anyhow!("{} terminated by signal {}",
                                    SYSTEMD_MAKEFS_COMMAND,
                                    status.signal().expect("on unix, status status.code() is None iff status.signal() isn't; \
                                                            this expect() will never panic, save for an stdlib bug"))),
            },
        Err(e) =>
            Err(e).with_context(|| {
                format!(
                    "{SYSTEMD_MAKEFS_COMMAND} call failed for /dev/{device_name}"
                )
            }),
    }
}

pub fn run_device_reset(device_name: &str) -> Result<()> {
    let reset = Path::new("/sys/block").join(device_name).join("reset");
    fs::write(reset, b"1")?;
    Ok(())
}
