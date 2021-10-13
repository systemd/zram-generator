/* SPDX-License-Identifier: MIT */

use zram_generator::{config, generator};

use anyhow::Result;
use fs_extra::dir::{copy, CopyOptions};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn prepare_directory(srcroot: &Path) -> Result<TempDir> {
    let rootdir = TempDir::new()?;
    let root = rootdir.path();

    let opts = CopyOptions::new();
    for p in vec!["etc", "usr", "proc"] {
        if srcroot.join(p).exists() {
            copy(srcroot.join(p), root, &opts)?;
        }
    }

    let output_directory = root.join("run/units");
    fs::create_dir_all(output_directory)?;

    Ok(rootdir)
}

fn test_generation(name: &str) -> Result<Vec<config::Device>> {
    let srcroot = Path::new(file!()).parent().unwrap().join(name);
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

    match name {
        "01-basic" => {
            assert_eq!(devices.len(), 1);
            let d = devices.iter().next().unwrap();
            assert!(d.is_swap());
            assert_eq!(d.host_memory_limit_mb, None);
            assert_eq!(d.zram_fraction, 0.5);
            assert_eq!(d.options, "discard");
        }

        "02-zstd" => {
            assert_eq!(devices.len(), 1);
            let d = devices.iter().next().unwrap();
            assert!(d.is_swap());
            assert_eq!(d.host_memory_limit_mb.unwrap(), 2050);
            assert_eq!(d.zram_fraction, 0.75);
            assert_eq!(d.compression_algorithm.as_ref().unwrap(), "zstd");
            assert_eq!(d.options, "discard");
        }

        "03-too-much-memory" => {
            assert_eq!(devices.len(), 0);
        }

        "04-dropins" => {
            assert_eq!(devices.len(), 2);

            for d in &devices {
                assert!(d.is_swap());

                match d.name.as_str() {
                    "zram0" => {
                        assert_eq!(d.host_memory_limit_mb.unwrap(), 1235);
                        assert_eq!(d.zram_fraction, 0.5);
                        assert_eq!(d.options, "discard");
                    }
                    "zram2" => {
                        assert!(d.host_memory_limit_mb.is_none());
                        assert_eq!(d.zram_fraction, 0.8);
                        assert_eq!(d.options, "");
                    }
                    _ => panic!("Unexpected device {}", d),
                }
            }
        }

        "05-kernel-disabled" => {
            assert_eq!(devices.len(), 0);
        }

        "06-kernel-enabled" => {
            assert_eq!(devices.len(), 1);
            let d = devices.iter().next().unwrap();
            assert!(d.is_swap());
            assert_eq!(d.host_memory_limit_mb, None);
            assert_eq!(d.zram_fraction, 0.5);
            assert_eq!(d.options, "discard");
        }

        "07-mount-point" => {
            assert_eq!(devices.len(), 2);
            for d in &devices {
                assert!(!d.is_swap());
                assert_eq!(d.host_memory_limit_mb, None);
                assert_eq!(d.zram_fraction, 0.5);
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
                    _ => panic!("Unexpected device {}", d),
                }
            }
        }

        "08-plain-device" => {
            assert_eq!(devices.len(), 1);
            let d = devices.iter().next().unwrap();
            assert!(!d.is_swap());
            assert_eq!(d.host_memory_limit_mb, None);
            assert_eq!(d.zram_fraction, 0.5);
            assert!(d.mount_point.is_none());
            assert_eq!(d.fs_type.as_ref().unwrap(), "ext2");
            assert_eq!(d.effective_fs_type(), "ext2");
            assert_eq!(d.options, "discard");
        }

        _ => (),
    }

    // Compare output directory to expected value.
    // ExecStart lines include the full path to the generating binary,
    // so exclude them from comparison.
    let diff = Command::new("diff")
        .arg("--recursive")
        .arg("--exclude=.empty")
        .arg("--ignore-matching-lines=^# Automatically generated by .*")
        .arg("--ignore-matching-lines=^ExecStart=/.* --setup-device '%i'")
        .arg("--ignore-matching-lines=^ExecStop=/.* --reset-device '%i'")
        .arg(srcroot.join("run.expected"))
        .arg(root.join("run"))
        .output()?;
    println!("stdout:\n{}", String::from_utf8_lossy(&diff.stdout));
    println!("stderr:\n{}", String::from_utf8_lossy(&diff.stderr));
    assert!(diff.status.success());

    Ok(devices)
}

#[test]
fn test_01_basic() {
    test_generation("01-basic").unwrap();
}

#[test]
fn test_02_zstd() {
    test_generation("02-zstd").unwrap();
}

#[test]
fn test_03_too_much_memory() {
    test_generation("03-too-much-memory").unwrap();
}

#[test]
fn test_04_dropins() {
    test_generation("04-dropins").unwrap();
}

#[test]
fn test_05_kernel_disabled() {
    test_generation("05-kernel-disabled").unwrap();
}

#[test]
fn test_06_kernel_enabled() {
    test_generation("06-kernel-enabled").unwrap();
}

#[test]
fn test_07_mount_point() {
    test_generation("07-mount-point").unwrap();
}

#[test]
fn test_08_plain_device() {
    test_generation("08-plain-device").unwrap();
}
