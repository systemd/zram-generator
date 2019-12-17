/* SPDX-License-Identifier: MIT */

use anyhow::{anyhow, Context, Result};
use crate::config::Device;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Command;


pub fn run_device_setup(device: Option<Device>, device_name: String) -> Result<()> {
    let device = device.ok_or_else(|| anyhow!("Device {} not found", device_name))?;

    let device_sysfs_path = Path::new("/sys/block").join(&device_name);
    let disksize_path = device_sysfs_path.join("disksize");
    fs::write(&disksize_path, format!("{}", device.disksize)).with_context(|| {
        format!(
            "Failed to configure disk size into {}",
            disksize_path.display()
        )
    })?;

    match Command::new("mkswap").arg(Path::new("/dev").join(&device_name)).status() {
        Ok(status) =>
            match status.code() {
                Some(0) => Ok(()),
                Some(code) => Err(anyhow!("mkswap failed with exit code {}", code)),
                None => Err(anyhow!("mkswap terminated by signal {}",
                                    status.signal().expect("on unix, status status.code() is None iff status.signal() isn't; \
                                                            this expect() will never panic, save for an stdlib bug"))),
            },
        Err(e) =>
            Err(e).with_context(|| {
                format!(
                    "mkswap call failed for /dev/{}",
                    device_name
                )
            }),
    }
}
