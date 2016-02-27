/// Run as an application, this is the starting point for our app

extern crate rustc_serialize;

use rustc_serialize::json::Json;
use std::slice::SliceConcatExt;

use std::process::Command;
use std::path::Path;
use std::vec::Vec;
use std::{env, fs};

pub enum ClippyState {
    Success,
    WithWarnings,
    WithErrors,
}

pub struct ClippyResult {
    pub ended: ClippyState,
    pub warnings: u8,
    pub errors: u8,
}

pub fn run<F>(path: &Path, logger: F) -> Result<ClippyResult, String>
    where F: Fn(&str)
{

    let libs_path = env::current_exe().unwrap();
    let libs_path = fs::canonicalize(libs_path).unwrap();
    let libs_path = libs_path.parent().unwrap();
    let libs_path = libs_path.join("deps");


    match Command::new("cargo")
              .args(&["rustc",
                      "--",
                      "-L",
                      &libs_path.to_string_lossy().into_owned(),
                      "-Zunstable-options",
                      "-Zextra-plugins=clippy",
                      "-Zno-trans",
                      "-lclippy",
                      "--error-format=json"])
              .current_dir(path)
              .output() {
        Ok(output) => {
            let mut warnings = 0;
            let mut errors = 0;
            let messages: Vec<String> = String::from_utf8(output.stderr)
                                            .unwrap()
                                            .split('\n')
                                            .filter_map(|line| Json::from_str(&line).ok())
                                            .filter_map(|json| {
                                                let obj = json.as_object().unwrap();
                                                match obj.get("level") {
                                                    Some(&Json::String(ref level)) => {
                                                        if level == "warning" {
                                                            warnings += 1;
                                                        } else if level == "error" {
                                                            errors += 1;
                                                        }
                                                        Some(format!("{level}: {msg}",
                                                                     level = level,
                                                                     msg = obj.get("message")
                                                                              .unwrap()))
                                                    }
                                                    _ => None,
                                                }
                                            })
                                            .collect();

            logger(&format!("Messages:\n {}", messages.join("\n")));

            if output.status.success() {
                match (errors, warnings) {
                    (0, 0) => {
                        Ok(ClippyResult {
                            ended: ClippyState::Success,
                            warnings: 0,
                            errors: 0,
                        })
                    }
                    (0, x) => {
                        Ok(ClippyResult {
                            ended: ClippyState::WithWarnings,
                            warnings: x,
                            errors: 0,
                        })
                    }
                    _ => {
                        Ok(ClippyResult {
                            ended: ClippyState::WithErrors,
                            warnings: warnings,
                            errors: errors,
                        })
                    }
                }
            } else {
                Err(format!("Clippy failed with Error code: {}",
                            output.status.code().unwrap_or(-999)))
            }
        }
        Err(error) => Err(format!("Running Clippy failed: {}", error)),
    }
}
