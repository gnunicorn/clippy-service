// Run Clippy in a firejail Sandbox and parse its output

extern crate rustc_serialize;

use rustc_serialize::json::Json;

use std::process::{ Command, Output };
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

fn run_in_sandbox<F>(path: &Path, command: &Vec<&str>, logger: &F) -> Output
    where F: Fn(&str)
{

    // Build the `firejail` arguments, using the preinstalled `cargo`-profile
    // use the `--force` flag to make it run even though we are in a docker
    // environment. For that to work, our docker needs to be setup to run in
    // the `--privileged` mode.

    logger(&format!("Running: {}", command.join(" ")));

    let str_path = path.to_string_lossy().into_owned();
    let whitelist = format!("--whitelist={}", &str_path);
    let name = format!("--name={}", &str_path);

    let mut args = vec!["--profile=/etc/firejail/cargo.profile",
                        "--force",
                        "--quiet",
                        whitelist.as_str(),
                        name.as_str(),

                        // Limit the resources each process is allowed to use to 75% of one CPU,
                        // for a maximum of 10 minutes soft (11min hard limit).
                        // Then add the command itself then run it
                        "timeout", "-k", "10m", "11m",
                        "cpulimit", "-l75", "--",

                        ];
    args.extend_from_slice(command);

    let output = Command::new("firejail")
                .args(&args)
                .current_dir(path)
                .output().unwrap_or_else( |e| {
                    logger(&format!("Running command failed :\n{}", &e));
                    panic!("failed to execute process: {}", &e);
              });

    // Log the content
    // and return the result
    logger(&format!("----- stdout:\n{}", &String::from_utf8(output.stdout.clone()).unwrap()));
    logger(&format!("----- stderr:\n{}", &String::from_utf8(output.stderr.clone()).unwrap()));

    // and return the result
    output
}

// ## Run
// Run clippy in the specified Path. We expect there to be a Cargo.toml file, checking for
// that should have happened already. While calculating the `ClippyResult`, inform about
// the current process via the `logger` function.
pub fn run<F>(path: &Path, logger: F) -> Result<ClippyResult, String>
    where F: Fn(&str)
{

    //  run rustc and cargo versions for easier debugging for the viewer
    run_in_sandbox(&path, &vec!["rustc", "--version"], &logger);
    run_in_sandbox(&path, &vec!["cargo", "--version"], &logger);

    logger("-------------------------------- Running Clippy");

    // Find the _local_ clippy we are shipping with the clippy-service
    // and append that to the cargo rustc process
    let libs_path = env::current_exe().unwrap();
    let libs_path = fs::canonicalize(libs_path).unwrap();
    let libs_path = libs_path.parent().unwrap();
    let libs_path = libs_path.join("deps");
    let libs_path = libs_path.to_str().unwrap().to_owned();


    let output = run_in_sandbox(&path, &vec![
                    // The command we want to run is `cargo rustc` with the extra compiler
                    // plugin for clippy which can be found at the library path passed after
                    // `-L`. Secondly we need rustc to report errors in the `json`-format (new
                    // nightly feature), so we can parse it later.
                    "cargo",
                    "rustc",
                    "--",
                    "-L",
                    libs_path.as_str(),
                    "-Zunstable-options",
                    "-Zextra-plugins=clippy",
                    "-Zno-trans",
                    "-lclippy",
                    "--error-format=json"], &logger);

    let stderr = String::from_utf8(output.stderr).unwrap();

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
                    Some(format!("{level}: {msg}", level = level,
                                 msg = obj.get("message").unwrap()))
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
    } else {
        Err("Running Clippy failed.".to_string())
    }
}
