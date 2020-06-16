use zram_generator::{config, generator};

use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn prepare_directory(srcroot: &Path) -> Result<TempDir> {
    let rootdir = TempDir::new()?;
    let root = rootdir.path();

    fs::create_dir_all(root.join("etc/systemd"))?;
    fs::copy(
        srcroot.join("etc/systemd/zram-generator.conf"),
        root.join("etc/systemd/zram-generator.conf"),
    )?;

    fs::create_dir_all(root.join("proc"))?;
    fs::copy(srcroot.join("proc/meminfo"), root.join("proc/meminfo"))?;

    let output_directory = root.join("run/units");
    fs::create_dir_all(output_directory)?;

    Ok(rootdir)
}

fn test_generation(name: &str) -> Result<Vec<config::Device>> {
    let srcroot = Path::new(file!()).parent().unwrap().join(name);
    let rootdir = prepare_directory(&srcroot)?;
    let root = rootdir.path();

    let devices = config::read_all_devices(root)?;

    let output_directory = root.join("run/units");
    generator::run_generator(&devices, &output_directory)?;

    match name {
        "01-basic" => {
            assert_eq!(devices.len(), 1);
            let d = devices.iter().next().unwrap();
            assert_eq!(d.host_memory_limit_mb, None);
            assert_eq!(d.zram_fraction, 0.5);
        }

        "02-zstd" => {
            assert_eq!(devices.len(), 1);
            let d = devices.iter().next().unwrap();
            assert_eq!(d.host_memory_limit_mb.unwrap(), 2050);
            assert_eq!(d.zram_fraction, 0.75);
            assert_eq!(d.compression_algorithm.as_ref().unwrap(), "zstd");
        }

        "03-too-much-memory" => {
            assert_eq!(devices.len(), 0);
        }

        _ => (),
    }

    // Compare output directory to expected value.
    // ExecStart lines include the full path to the generating binary,
    // so exclude them from comparison.
    let diff = Command::new("diff")
        .arg("--recursive")
        .arg("--exclude=.empty")
        .arg("--ignore-matching-lines=ExecStart=/.* --setup-device '%i'")
        .arg("--ignore-matching-lines=ExecStop=/.* --reset-device '%i'")
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
