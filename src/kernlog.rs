//! Logger implementation for low level kernel log (using `/dev/kmsg`)
//!
//! Borrowed and cut down from https://github.com/kstep/kernlog.rs/pull/2,
//! consider merging changes back when fixing something here;
//! this automatically falls back to stdout and ignores problems with opening "/dev/kmsg".

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::sync::Mutex;

/// Kernel logger implementation
pub struct KernelLog {
    kmsg: Mutex<Option<File>>,
    maxlevel: log::LevelFilter,
}

impl KernelLog {
    /// Create new kernel logger with error level filter
    pub fn with_level(level: log::LevelFilter) -> KernelLog {
        KernelLog {
            kmsg: Mutex::new(OpenOptions::new().write(true).open("/dev/kmsg").ok()),
            maxlevel: level,
        }
    }
}

impl log::Log for KernelLog {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        meta.level() <= self.maxlevel
    }

    fn log(&self, record: &log::Record) {
        if record.level() > self.maxlevel {
            return;
        }

        let level: u8 = match record.level() {
            log::Level::Error => 3,
            log::Level::Warn => 4,
            log::Level::Info => 5,
            log::Level::Debug => 6,
            log::Level::Trace => 7,
        };

        let mut buf = Vec::new();
        writeln!(
            buf,
            "<{}>{}[{}]: {}",
            level,
            record.target(),
            unsafe { libc::getpid() },
            record.args()
        )
        .unwrap();

        if let Ok(mut kmsg) = self.kmsg.lock() {
            match kmsg.as_mut() {
                Some(kmsg) => {
                    let _ = kmsg.write(&buf);
                    let _ = kmsg.flush();
                }
                None => {
                    let kmsg = io::stdout();
                    let mut kmsg = kmsg.lock();
                    let _ = kmsg.write(&buf);
                    let _ = kmsg.flush();
                }
            }
        }
    }

    fn flush(&self) {}
}

/// Setup kernel logger with specified error level as the default logger
pub fn init_with_level(level: log::LevelFilter) -> Result<(), log::SetLoggerError> {
    log::set_boxed_logger(Box::new(KernelLog::with_level(level)))?;
    log::set_max_level(level);
    Ok(())
}
