/* SPDX-License-Identifier: MIT */

use zram_generator::{config, generator};
use anyhow::{Context, Result};
use fs_extra::dir::{copy, CopyOptions};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::process::{exit, Command};
use tempfile::TempDir;

#[ctor::ctor]
fn unshorn() {
    use nix::{errno, mount, sched, unistd};
    use std::os::unix::fs::symlink;

    let (uid, gid) = (unistd::geteuid(), unistd::getegid());
    if !uid.is_root() {
        match sched::unshare(sched::CloneFlags::CLONE_NEWUSER) {
            Err(errno::Errno::EPERM) => {
                eprintln!("unshare(NEWUSER) forbidden and not running as root: skipping tests");
                exit(0);
            }
            r => r.expect("unshare(NEWUSER)"),
        }

        fs::write("/proc/self/setgroups", b"deny")
            .context("Failed to write to /proc/self/setgroups")
            .unwrap_or_else(|e| {
                eprintln!("{}", e);
                exit(1);
            });

        fs::write("/proc/self/uid_map", format!("0 {} 1", uid))
            .context("Failed to write to /proc/self/uid_map")
            .unwrap_or_else(|e| {
                eprintln!("{}", e);
                exit(1);
            });

        fs::write("/proc/self/gid_map", format!("0 {} 1", gid))
            .context("Failed to write to /proc/self/gid_map")
            .unwrap_or_else(|e| {
                eprintln!("{}", e);
                exit(1);
            });
    }

    sched::unshare(sched::CloneFlags::CLONE_NEWNS)
        .context("Failed to unshare namespace (NEWNS)")
        .unwrap_or_else(|e| {
            eprintln!("{}", e);
            exit(1);
        });

    mount::mount::<_, _, str, str>(
        Some("none"),
        "/",
        None,
        mount::MsFlags::MS_REC | mount::MsFlags::MS_PRIVATE,
        None,
    )
    .context("Failed to remount root as private")
    .unwrap_or_else(|e| {
        eprintln!("{}", e);
        exit(1);
    });

    mount::mount::<str, _, _, str>(None, "/proc", Some("tmpfs"), mount::MsFlags::empty(), None)
        .context("Failed to mount /proc as tmpfs")
        .unwrap_or_else(|e| {
            eprintln!("{}", e);
            exit(1);
        });

    fs::create_dir("/proc/self")
        .context("Failed to create /proc/self directory")
        .unwrap_or_else(|e| {
            eprintln!("{}", e);
            exit(1);
        });

    symlink("zram-generator", "/proc/self/exe")
        .context("Failed to create symlink for /proc/self/exe")
        .unwrap_or_else(|e| {
            eprintln!("{}", e);
            exit(1);
        });

    let mut path = env::var_os("PATH")
        .map(|p| p.to_os_string().into_vec())
        .unwrap_or_else(|| b"/usr/bin:/bin".to_vec()); // _PATH_DEFPATH
    path.insert(0, b':');
    for &b in "tests/10-example/bin".as_bytes().iter().rev() {
        path.insert(0, b);
    }
    env::set_var("PATH", OsString::from_vec(path));
}

fn prepare_directory(srcroot: &Path) -> Result<TempDir> {
    let rootdir = TempDir::new().context("Failed to create temporary directory")?;
    let root = rootdir.path();

    let opts = CopyOptions::new();
    for p in ["etc", "usr", "proc"]
        .iter()
        .map(|p| srcroot.join(p))
        .filter(|p| p.exists())
    {
        copy(&p, root, &opts).with_context(|| format!("Failed to copy {:?}", p))?;
    }

    let output_directory = root.join("run/units");
    fs::create_dir_all(&output_directory)
        .context("Failed to create output directory for run/units")?;

    Ok(rootdir)
}

fn test_generation(path: &str) -> Result<Vec<config::Device>> {
    let srcroot = Path::new(path);
    let rootdir = prepare_directory(&srcroot)?;
    let root = rootdir.path();

    let kernel_override = match config::kernel_zram_option(root) {
        Some(true) => true,
        Some(false) => {
            return Ok(vec![]);
        }
        _ => false,
    };
    let devices = config::read_all_devices(root, kernel_override)
        .context("Failed to read devices from configuration")?;

    let output_directory = root.join("run/units");
    generator::run_generator(&devices, &output_directory, true)
        .context("Failed to run generator")?;

    // Compare output directory to expected value.
    let diff = Command::new("diff")
        .arg("-u")
        .arg("--recursive")
        .arg("--exclude=.empty")
        .arg(srcroot.join("run.expected"))
        .arg(root.join("run"))
        .output()
        .context("Failed to execute diff command")?;

    for (h, d) in [("stdout", &diff.stdout), ("stderr", &diff.stderr)] {
        if !d.is_empty() {
            println!("{}:{}", h, String::from_utf8_lossy(d));
        }
    }
    if !diff.status.success() {
        anyhow::bail!("diff command failed");
    }

    Ok(devices)
}
