// Run Clippy in a firejail Sandbox and parse its output

extern crate rustc_serialize;

use rustc_serialize::json::Json;

use std::process::Command;
use std::path::Path;
use std::vec::Vec;
use std::{env, fs};

// Enum describing the State of the Clippy result,
// whether everything went fine or if warnings or
// errors were found – and if so, how many
pub enum ClippyResult {
    Success,
    WithWarnings(u32),
    WithErrors(u32, u32),
}

// ## Run
// Run clippy in the specified Path. We expect there to be a Cargo.toml file, checking for
// that should have happened already. While calculating the `ClippyResult`, inform about
// the current process via the `logger` function.
pub fn run<F>(path: &Path, logger: F) -> Result<ClippyResult, String>
    where F: Fn(&str)
{

    // Find the _local_ clippy we are shipping with the clippy-service
    // and append that to the cargo rustc process
    let libs_path = env::current_exe().unwrap();
    let libs_path = fs::canonicalize(libs_path).unwrap();
    let libs_path = libs_path.parent().unwrap();
    let libs_path = libs_path.join("deps");

    // Start the the `firejail` process, using the preinstalled `cargo`-profile
    // use the `--force` flag to make it run even though we are in a docker
    // environment. For that to work, our docker needs to be setup to run in
    // the `--privileged` mode.
    // Lastly allow it to access the usually unaccessible dependencies, where
    // our clippy lib is stored.
    match Command::new("firejail")
              .args(&["--profile=/etc/firejail/cargo.profile",
                      "--force",
                      format!("--whitelist={}",
                      &path.to_string_lossy().into_owned()).as_str(),

    // The command we want to run is `cargo rustc` with the extra compiler
    // plugin for clippy which can be found at the library path passed after
    // `-L`. Secondly we need rustc to report errors in the `json`-format (new
    // nightly feature), so we can parse it later.
                      "timeout", "-k", "10m", "11m",
                      "cpulimit", "-l75", "--",
                      "cargo",
                      "rustc",
                      "--",
                      "-L",
                      &libs_path.to_string_lossy().into_owned(),
                      "-Zunstable-options",
                      "-Zextra-plugins=clippy",
                      "-Zno-trans",
                      "-lclippy",
                      "--error-format=json"])
    // Run it from the directory passed in and keep the output
              .current_dir(path)
              .output() {
        Ok(output) => {
            // First and foremost: read and log the output, so that
            // humans looking at it can use it. See how we are using the
            // logger-function to do that?
            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();
            logger(&format!("----- stdout:\n{}", &stdout));
            logger(&format!("----- stderr:\n{}", &stderr));
            let mut warnings = 0;
            let mut errors = 0;
            // Next up, we need to parse the outpuf from stderr, where
            // clippy and the compiler might report errors to us. There is
            // one error per line, which is why we split it into lines. We
            // then use `filter_map` to find all those lines we can decode
            // from JSON
            let messages: Vec<String> = stderr.split('\n')
                                            .filter_map(|line| Json::from_str(&line).ok())
                                            .filter_map(|json| {
                                                // and then `filter_map` those into
                                                // the warnings and errors we care about,
                                                // while also updating the local count
                                                // of both.
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
                                            // The collect executes this iterative into
                                            // a vector of results. We can now log to the
                                            // viewer.
                                            .collect();

            logger(&format!("-----\nMessages identified:\n {}", messages.join("\n")));

            // Next parse the count of errors and warnings
            // and wrap that into the appropriate `ClippyResult`
            if output.status.success() {
                match (errors, warnings) {
                    (0, 0) => Ok(ClippyResult::Success),
                    (0, x) => Ok(ClippyResult::WithWarnings(x)),
                    _ => Ok(ClippyResult::WithErrors(errors, warnings))
                }
            // Or report an Error if clippy (or firejail) failed to execute
            } else {
                Err(format!("Clippy failed with Error code: {}",
                            output.status.code().unwrap_or(-999)))
            }
        }
        Err(error) => Err(format!("Running Clippy failed: {}", error)),
    }
}
