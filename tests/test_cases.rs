/* SPDX-License-Identifier: MIT */

use zram_generator::{config, generator};

use anyhow::Result;
use fs_extra::dir::{copy, CopyOptions};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

#[ctor::ctor]
fn unshorn() {
    use nix::{mount, sched, unistd};
    use std::os::unix::fs::symlink;

    let (uid, gid) = (unistd::geteuid(), unistd::getegid());
    sched::unshare(sched::CloneFlags::CLONE_NEWUSER | sched::CloneFlags::CLONE_NEWNS)
        .expect("unshare(NEWUSER | NEWNS)");
    fs::write("/proc/self/setgroups", b"deny").unwrap();
    fs::write("/proc/self/uid_map", format!("0 {} 1", uid)).unwrap();
    fs::write("/proc/self/gid_map", format!("0 {} 1", gid)).unwrap();

    mount::mount::<str, _, _, str>(None, "/proc", Some("tmpfs"), mount::MsFlags::empty(), None)
        .unwrap();
    fs::create_dir("/proc/self").unwrap();
    symlink("zram-generator", "/proc/self/exe").unwrap();
}

fn prepare_directory(srcroot: &Path) -> Result<TempDir> {
    let rootdir = TempDir::new()?;
    let root = rootdir.path();

    let opts = CopyOptions::new();
    for p in ["etc", "usr", "proc"]
        .iter()
        .map(|p| srcroot.join(p))
        .filter(|p| p.exists())
    {
        copy(p, root, &opts)?;
    }

    let output_directory = root.join("run/units");
    fs::create_dir_all(output_directory)?;

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
    let devices = config::read_all_devices(root, kernel_override)?;

    let output_directory = root.join("run/units");
    generator::run_generator(&devices, &output_directory, true)?;

    // Compare output directory to expected value.
    // ExecStart lines include the full path to the generating binary,
    // so exclude them from comparison.
    let diff = Command::new("diff")
        .arg("-u")
        .arg("--recursive")
        .arg("--exclude=.empty")
        .arg(srcroot.join("run.expected"))
        .arg(root.join("run"))
        .output()?;
    for (h, d) in [("stdout", &diff.stdout), ("stderr", &diff.stderr)] {
        if !d.is_empty() {
            println!("{}:{}", h, String::from_utf8_lossy(d));
        }
    }
    assert!(diff.status.success());

    Ok(devices)
}

fn z_s_name(zram_size: &(String, fasteval::ExpressionI, fasteval::Slab)) -> &str {
    &zram_size.0
}

#[test]
fn test_01_basic() {
    let devices = test_generation("tests/01-basic").unwrap();
    assert_eq!(devices.len(), 1);
    let d = &devices[0];
    assert!(d.is_swap());
    assert_eq!(d.host_memory_limit_mb, None);
    assert_eq!(d.zram_size.as_ref().map(z_s_name), None);
    assert_eq!(d.options, "discard");
}

#[test]
fn test_02_zstd() {
    let devices = test_generation("tests/02-zstd").unwrap();
    assert_eq!(devices.len(), 1);
    let d = &devices[0];
    assert!(d.is_swap());
    assert_eq!(d.host_memory_limit_mb, Some(2050));
    assert_eq!(d.zram_size.as_ref().map(z_s_name), Some("ram * 0.75"));
    assert_eq!(d.compression_algorithm.as_ref().unwrap(), "zstd");
    assert_eq!(d.options, "discard");
}

#[test]
fn test_03_too_much_memory() {
    let devices = test_generation("tests/03-too-much-memory").unwrap();
    assert_eq!(devices.len(), 0);
}

#[test]
fn test_04_dropins() {
    let devices = test_generation("tests/04-dropins").unwrap();
    assert_eq!(devices.len(), 2);

    for d in &devices {
        assert!(d.is_swap());

        match &d.name[..] {
            "zram0" => {
                assert_eq!(d.host_memory_limit_mb, Some(1235));
                assert_eq!(d.zram_size.as_ref().map(z_s_name), None);
                assert_eq!(d.options, "discard");
            }
            "zram2" => {
                assert_eq!(d.host_memory_limit_mb, None);
                assert_eq!(d.zram_size.as_ref().map(z_s_name), Some("ram*0.8"));
                assert_eq!(d.options, "");
            }
            _ => panic!("Unexpected device {}", d),
        }
    }
}

#[test]
fn test_05_kernel_disabled() {
    let devices = test_generation("tests/05-kernel-disabled").unwrap();
    assert_eq!(devices.len(), 0);
}

#[test]
fn test_06_kernel_enabled() {
    let devices = test_generation("tests/06-kernel-enabled").unwrap();
    assert_eq!(devices.len(), 1);
    let d = &devices[0];
    assert!(d.is_swap());
    assert_eq!(d.host_memory_limit_mb, None);
    assert_eq!(d.zram_size.as_ref().map(z_s_name), None);
    assert_eq!(d.options, "discard");
}

#[test]
fn test_07_mount_point() {
    let devices = test_generation("tests/07-mount-point").unwrap();
    assert_eq!(devices.len(), 4);
    test_07_devices(devices);
}

/// cargo-package refuses to pack files with `\`s in them,
/// so we split them off to be able to push to crates.io
#[test]
fn test_07a_mount_point_excl() {
    if !Path::new("tests/07a-mount-point-excl").exists() {
        io::stdout()
            .write_all(b"07a-mount-point-excl doesn't exist: assuming package, skipping\n")
            .unwrap();
        return;
    }

    let devices = test_generation("tests/07a-mount-point-excl").unwrap();
    assert_eq!(devices.len(), 1);
    test_07_devices(devices);
}

fn test_07_devices(devices: Vec<config::Device>) {
    for d in &devices {
        assert!(!d.is_swap());
        assert_eq!(d.host_memory_limit_mb, None);
        assert_eq!(d.zram_size.as_ref().map(z_s_name), None);
        assert_eq!(d.fs_type.as_ref().unwrap(), "ext4");
        assert_eq!(d.effective_fs_type(), "ext4");
        match &d.name[..] {
            "zram11" => {
                assert_eq!(
                    d.mount_point.as_ref().unwrap(),
                    Path::new("/var/compressed")
                );
                assert_eq!(d.options, "discard");
            }
            "zram12" => {
                assert_eq!(d.mount_point.as_ref().unwrap(), Path::new("/var/folded"));
                assert_eq!(d.options, "discard,casefold");
            }
            "zram13" => {
                assert_eq!(d.mount_point.as_ref().unwrap(), Path::new("/foo//bar/baz/"));
                assert_eq!(d.options, "discard");
            }
            "zram14" => {
                assert_eq!(d.mount_point.as_ref().unwrap(), Path::new("/.żupan-ci3pły"));
                assert_eq!(d.options, "discard");
            }
            "zram15" => {
                assert_eq!(d.mount_point.as_ref().unwrap(), Path::new("///"));
                assert_eq!(d.options, "discard");
            }
            _ => panic!("Unexpected device {}", d),
        }
    }
}

#[test]
fn test_08_plain_device() {
    let devices = test_generation("tests/08-plain-device").unwrap();
    assert_eq!(devices.len(), 1);
    let d = &devices[0];
    assert!(!d.is_swap());
    assert_eq!(d.host_memory_limit_mb, None);
    assert_eq!(d.zram_size.as_ref().map(z_s_name), None);
    assert!(d.mount_point.is_none());
    assert_eq!(d.fs_type.as_ref().unwrap(), "ext2");
    assert_eq!(d.effective_fs_type(), "ext2");
    assert_eq!(d.options, "discard");
}

#[test]
fn test_09_zram_size() {
    let devices = test_generation("tests/09-zram-size").unwrap();
    assert_eq!(devices.len(), 1);
    let d = &devices[0];
    assert!(d.is_swap());
    assert_eq!(d.host_memory_limit_mb, Some(2050));
    assert_eq!(
        d.zram_size.as_ref().map(z_s_name),
        Some("min(0.75 * ram, 6000)")
    );
    assert_eq!(d.compression_algorithm.as_ref().unwrap(), "zstd");
}

#[test]
fn test_10_example() {
    if !Path::new("tests/10-example").exists() {
        io::stdout()
            .write_all(b"10-example doesn't exist: assuming package, skipping\n")
            .unwrap();
        return;
    }

    let devices = test_generation("tests/10-example").unwrap();
    assert_eq!(devices.len(), 2);

    for d in &devices {
        match d.name.as_str() {
            "zram0" => {
                assert!(d.is_swap());
                assert_eq!(d.host_memory_limit_mb, Some(9048));
                assert_eq!(
                    d.zram_size.as_ref().map(z_s_name),
                    Some("min(ram / 10, 2048)")
                );
                assert_eq!(d.compression_algorithm.as_deref(), Some("lzo-rle"));
                assert_eq!(d.options, "");
            }
            "zram1" => {
                assert_eq!(d.fs_type.as_ref().unwrap(), "ext2");
                assert_eq!(d.effective_fs_type(), "ext2");
                assert_eq!(d.zram_size.as_ref().map(z_s_name), Some("ram / 10"));
                assert_eq!(d.options, "discard");
            }
            _ => panic!("Unexpected device {}", d),
        }
    }
}

#[test]
fn test_11_obsolete() {
    let devices = test_generation("tests/11-obsolete").unwrap();
    assert_eq!(devices.len(), 2);

    for d in &devices {
        assert!(d.is_swap());
        assert_eq!(d.options, "discard");
        match d.name.as_str() {
            "zram0" => {
                assert_eq!(d.host_memory_limit_mb, Some(100000));
                assert_eq!(d.zram_fraction, Some(0.1));
                assert_eq!(d.max_zram_size_mb, Some(Some(2048)));
            }
            "zram1" => {
                assert_eq!(d.host_memory_limit_mb, None);
                assert_eq!(d.zram_fraction, Some(0.1));
                assert_eq!(d.max_zram_size_mb, Some(None));
            }
            _ => panic!("Unexpected device {}", d),
        }
    }
}
