// Github Specific Backend code
extern crate redis;
extern crate time;
extern crate tempdir;
extern crate rand;

use std::path::Path;
use std::thread;
use tempdir::TempDir;

use redis::{Commands, PipelineCommands};

use helpers::{setup_redis, log_redis, download_and_unzip};
use clippy::{ClippyResult, run as run_clippy};

// ## Update For Github
// Given the user, repo and SHA, this function fetches the
// git repo and runs clippy in the folder containing the `Cargo.toml` file
// This is the internal function invoked from `schedule_update` in a seperat
// background thread. It will return an Error or the ClippyResult once done.
fn update_for_github<F>(user: &str,
                        repo: &str,
                        sha: &str,
                        logger: F)
                        -> Result<ClippyResult, String>
// One interesting feature of this function (and a few others) is the scoped
// `logger` which is passed around. During execution this function is invoked
// multiple times to report on the current state of affairs.
    where F: Fn(&str)
{
    // We start by creating a temporary directory for our checkout
    logger("Creating Temp Directory...");

    if let Ok(temp_dir) = TempDir::new(&format!("github_{0}_{1}_{2}", user, repo, sha)) {

        // Then we need to download the ZIP-Archive for the given user-repo-sha.
        // Github has a handy URL to do that directly, which we just pass to the
        // `download_and_unzip` function.
        let github_url = format!("https://codeload.github.com/{0}/{1}/zip/{2}",
                                 user,
                                 repo,
                                 sha);


        logger(&format!("Fetching {}", &github_url));
        match download_and_unzip(&github_url, &temp_dir) {
            Ok(files) => {
                // Once unzipped, we report back the files found and try to find the
                // patch containing the 'cargo.toml' file – this iter stops at the first
                // item found.
                logger(&format!("Extracted: \n - {}", files.join("\n - ")));
                match files.iter().find(|item| item.to_lowercase().ends_with("cargo.toml")) {
                    Some(file) => {
                        // Look up the bounding directory for that file, report
                        // that path and execute `run_clippy` in that folder
                        let path = Path::new(file);
                        let parent_directory = path.parent().unwrap();
                        logger(&format!("Cargo file found in {}",
                                        parent_directory.to_string_lossy().into_owned()));
                        run_clippy(parent_directory, logger)
                    }
                    // Report back if there is no `Cargo.toml` file or if there has been
                    // any other error during download_and_unzip.
                    _ => Err(String::from("No `Cargo.toml` file found in archive.")),
                }
            }
            Err(err) => Err(err),
        }
    } else {
        // We could run into some IO error, causing the temporary directory creation to
        // fail. Report that appropriately.
        Err(String::from("Creating temp directory failed"))
    }
}

// ## Schedule Update
// Given the username, repo and SHA from Github, this public function
// will schedule the fetching and running of clippy in a background thread.
pub fn schedule_update(user: &str, repo: &str, sha: &str) {

    // Setup the scope for the background thread. We need to move all
    // variables here to ensure they can't change during thread runtime.
    let user = user.to_owned();
    let repo = repo.to_owned();
    let sha = sha.to_owned();
    let base_key = format!("github/{0}/{1}:{2}", user, repo, sha).to_owned();

    let result_key = format!("result/{}", base_key).to_owned();
    let lock_key = format!("lock/{}", base_key).to_owned();
    let log_key = format!("log/{}", base_key).to_owned();

    // now spawn the background thread. We create both the redis connection
    // and the logger clojure in here to avoid ownership problems.
    thread::spawn(move || {
        let redis: redis::Connection = setup_redis();
        let logger = |statement: &str| log_redis(&redis, &log_key, statement);

        // Processes could be scheduled at the same time. We use redis to keep
        // a shared lock to ensure we aren't running the same process and write
        // to the same log more than once.
        // See http://redis.io/topics/distlock#correct-implementation-with-a-single-instance
        // to learn more about using redis with distributed locks.

        let random_lock_value : u32 = rand::random::<u32>();
        let _ : bool = redis.set_nx(lock_key.clone(), random_lock_value).unwrap();
        let lock_val : u32 = redis.get(lock_key.clone()).unwrap();
        if  lock_val != random_lock_value {
            // we aren't the ones, who acquired the lock, means someone
            // else is running this process. We should quit immediately.
            return
        }

        // Make sure we expire the key though – in 15min.
        let _ : bool = redis.expire(lock_key.clone(), 900).unwrap();

        // No background thread yet, we are ready to roll: execute `update_for_github`
        // and parse the result. If there is ClippyResult, match it to the appropriate
        // status output, otherwise, report the error and set the status to "failed".

        logger("Started Processing");

        let text: String = match update_for_github(&user, &repo, &sha, logger) {
            Ok(result) => {
                match result {
                    ClippyResult::Success => String::from("success"),
                    ClippyResult::WithWarnings(warnings) => format!("{0} warnings", warnings),
                    ClippyResult::WithErrors(errors, _) => format!("{0} errors", errors)
                }
            }
            Err(error) => {
                log_redis(&redis, &log_key, &format!("Failed: {}", error));
                String::from("failed")
            }
        };

        // log the output from clippy and set the result into the redis cache.
        // we are done with our background thread. Rust will take care of cleaning
        // up for us here automatically.
        log_redis(&redis,
                  &log_key,
                  &format!("------------------------------------------\n Clippy's final \
                            verdict: {}",
                           text));
        redis::pipe()
            .cmd("SET")
            .arg(result_key)
            .arg(text.clone())
            .ignore()
            .execute(&redis);
    });
}
