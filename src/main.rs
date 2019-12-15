/* SPDX-License-Identifier: MIT */

mod config;
mod generator;
mod setup;

use config::Config;

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    match Config::parse() {
        Ok(config) => {
            if config.devices.is_empty() {
                println!("No devices configured, exiting.");
                return 0;
            }

            match config.run() {
                Ok(()) => 0,
                Err(e) => {
                    println!("{}", e);
                    2
                }
            }
        },
        Err(e) => {
            println!("{}", e);
            1
        },
    }
}
