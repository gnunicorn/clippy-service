/// Run as an application, this is the starting point for our app
extern crate redis;
extern crate time;
extern crate tempdir;

use std::path::Path;
use std::thread;
use tempdir::TempDir;
use time::now_utc;

use std::slice::SliceConcatExt;
use redis::{Commands, PipelineCommands};

use helpers::{setup_redis, log_redis, download_and_unzip};
use clippy::{ClippyState, ClippyResult, run as run_clippy};

static LINTING_BADGE_URL: &'static str = "https://img.shields.io/badge/clippy-linting-blue";

fn update_for_github<F>(user: &str, repo: &str, sha: &str, logger: F) -> Result<ClippyResult, String>
    where F : Fn(&str) {
    logger("Creating Temp Directory...");

    if let Ok(temp_dir) = TempDir::new(&format!("github_{0}_{1}_{2}",
                                                user,
                                                repo,
                                                sha)) {

        let github_url = format!("https://codeload.github.com/{0}/{1}/zip/{2}",
                                 user,
                                 repo,
                                 sha);


        logger(&format!("Fetching {}", &github_url));
        match download_and_unzip(&github_url, &temp_dir) {
            Ok(files) => {
                logger(&format!("Extracted: \n - {}", files.join("\n - ")));
                match files.iter().find(|item| item.to_lowercase().ends_with("cargo.toml")) {
                    Some(file) => {
                        let path = Path::new(file);
                        let parent_directory = path.parent().unwrap();
                        logger(&format!("Cargo file found in {}", parent_directory .to_string_lossy().into_owned()));
                        logger("-------------------------------- Running Clippy");
                        run_clippy(parent_directory, logger)
                    }
                    _ => Err(String::from("No `Cargo.toml` file found in archive."))
                }
            },
            Err(err) => Err(err)
        }
    } else {
        Err(String::from("Creating temp directory failed"))
    }
}

pub fn schedule_update(user: &str, repo: &str, sha: &str){

    let user = user.to_owned();
    let repo = repo.to_owned();
    let sha = sha.to_owned();
    let base_key = format!("github/{0}/{1}:{2}",
                            user,
                            repo,
                            sha).to_owned();

    let result_key = format!("result/{}", base_key).to_owned();
    let log_key = format!("log/{}", base_key).to_owned();
    let badge_key = format!("badge/{}", base_key).to_owned();

    thread::spawn(move || {
        let redis: redis::Connection = setup_redis();
        let logger = |statement: &str| log_redis(&redis, &log_key, statement);

        // let's make sure we are the first to run here,
        // otherwise, exit the thread preemptively
        if let Ok(existing) = redis::transaction(&redis, &[log_key.clone(), badge_key.clone()], |pipe| {
            match redis.exists(badge_key.clone()) {
                Ok(Some(false)) => {
                    pipe.cmd("RPUSH")
                            .arg(log_key.clone())
                            .arg(format!("{0} started processing github/{1}/{2}:{3}",
                                                now_utc().rfc3339(),
                                                user,
                                                repo,
                                                sha))
                            .ignore()
                        .cmd("SET")
                            .arg(badge_key.clone())
                            .arg(LINTING_BADGE_URL)
                            .ignore()
                        .cmd("EXPIRE")
                            .arg(badge_key.clone())
                            .arg(300)
                            .ignore()
                        .execute(&redis);
                    Ok(Some(false))
                    },
                _ => Ok(Some(true))
            }}) {
            if existing {
                // we have been alerted, the key already existed
                // so someone else is writing a log file. We should stop now.
                return;
            }
        }


        let (text, color) : (String, &str) = match update_for_github(&user, &repo, &sha, logger) {
            Ok(result) => {
                match result.ended {
                    ClippyState::Success => (String::from("success"), "brightgreen"),
                    ClippyState::WithWarnings => (
                            format!("{0} warnings", result.warnings),
                            match result.warnings {
                                1...5 => "yellowgreen",
                                5...10 => "yellow",
                                10...50 => "orange",
                                _ => "red"
                            }),
                    ClippyState::WithErrors => (
                            format!("{0} errors", result.errors),
                            "red")
                }
            },
            Err(error) => {
                log_redis(&redis, &log_key, &format!("Failed: {}", error));
                (String::from("failed"), "red")
            }
        };

        log_redis(&redis, &log_key, &format!("------------------------------------------\n Clippy's final verdict: {}", text));
        redis::pipe()
            .cmd("SET")
                .arg(result_key)
                .arg(text.clone())
                .ignore()
            .cmd("SET")
                .arg(badge_key)
                .arg(format!("https://img.shields.io/badge/clippy-{}-{}", text, color))
                .ignore()
            .execute(&redis);
    });
}
