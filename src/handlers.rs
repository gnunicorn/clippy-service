/// Run as an application, this is the starting point for our app
extern crate iron;
extern crate staticfile;
extern crate redis;
extern crate rustc_serialize;
extern crate hyper;
extern crate url;
extern crate time;
extern crate tempdir;
extern crate zip;

extern crate router;
extern crate mime;

use std::path::Path;
use std::vec::Vec;
use std::thread;
use std::process::Command;
use tempdir::TempDir;
use time::now_utc;

use rustc_serialize::json::Json;

use iron::modifiers::Redirect;
use iron::prelude::*;
use iron::status;
use iron::Url as iUrl;

use url::Url;

use hyper::client::Client;

use router::Router;

use std::slice::SliceConcatExt;
use redis::{Commands, PipelineCommands, Value};

pub enum ClippyState {
    EndedFine,
    EndedWithWarnings,
    EndedWithErrors
}

struct ClippyResult {
    state: ClippyState,
    warnings: u8,
    errors: u8,
}

use helpers::{setup_redis, log_redis, fetch, redir, set_redis_cache, download_and_unzip};

static LINTING_BADGE_URL: &'static str = "https://img.shields.io/badge/clippy-linting-blue";

fn run_clippy<F>(path: &Path, logger: F) -> Result<ClippyResult, String>
    where F : Fn(&str) {
    match Command::new("cargo")
                .args(&["rustc", "--", "-Zunstable-options",
                        "-Zextra-plugins=clippy", "-Zno-trans",
                        "-lclippy", "--error-format=json"])
                  .current_dir(path)
                  .output() {
        Ok(output) => {
            let mut warnings = 0;
            let mut errors = 0;
            let messages: Vec<String> = String::from_utf8(output.stderr)
                                  .unwrap_or(String::from(""))
                                  .split("\n")
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
                                              Some(format!("{level}: {msg}", level=level, msg=obj.get("message").unwrap()))
                                          },
                                          _ => None
                                      }
                                  })
                                  .collect();

            logger(&format!("Messages:\n {}", messages.join("\n")));

            if output.status.success() {
                match (errors, warnings) {
                    (0, 0) => Ok(ClippyResult{state: ClippyState::EndedFine,
                                        warnings: 0,
                                        errors: 0}),
                    (0, x) => Ok(ClippyResult{state: ClippyState::EndedWithWarnings,
                                        warnings: x,
                                        errors: 0}),
                    _ => Ok(ClippyResult{state: ClippyState::EndedWithErrors,
                                        warnings: warnings,
                                        errors: errors})
                }
            } else {
                Err(format!("Clippy failed with Error code: {}", output.status.code().unwrap_or(-999)))
            }
        },
        Err(error) => Err(format!("Running Clippy failed: {}", error))
    }
}

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

fn trigger_update(user: &str, repo: &str, sha: &str){

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
                    return Ok(Some(false));
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
                match result.state {
                    ClippyState::EndedFine => (String::from("success"), "brightgreen"),
                    ClippyState::EndedWithWarnings => (
                            format!("{0} warnings", result.warnings),
                            match result.warnings {
                                1...5 => "yellowgreen",
                                5...10 => "yellow",
                                10...50 => "orange",
                                _ => "red"
                            }),
                    ClippyState::EndedWithErrors => (
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


pub fn github_finder(req: &mut Request) -> IronResult<Response> {

    // expand a branch name into the hash, keep the redirect for 5min

    let ref router = req.extensions.get::<Router>().unwrap();
    let redis: redis::Connection = setup_redis();
    let hyper_client: Client = Client::new();

    let user = router.find("user").unwrap();
    let repo = router.find("repo").unwrap();
    let branch = router.find("branch").unwrap_or("master");
    let method = router.find("method").unwrap_or("badge.svg");

    let redis_key = format!("cached-sha/github/{0}/{1}:{2}", user, repo, branch);
    let mut target_url = req.url.clone().into_generic_url().to_owned();

    match redis.get(redis_key.to_owned()){
        // we have a cached value, redirect directly
        Ok(Value::Data(sha)) =>{
            {
                let mut path = target_url.path_mut().unwrap();
                path.clear();
                path.extend_from_slice(&["github".to_owned(), "sha".to_owned(), user.to_owned(), repo.to_owned(), String::from_utf8(sha).unwrap().to_owned(), method.to_owned()]);
            }
            return redir(&target_url, &req.url);
        },
        _ => {
            let github_url = format!("https://api.github.com/repos/{0}/{1}/git/refs/heads/{2}",
                                     user,
                                     repo,
                                     branch);
            if let Some(body) = fetch(&hyper_client, &github_url) {
                if let Ok(json) = Json::from_str(&body) {
                    if let Some(&Json::String(ref sha)) = json.find_path(&["object", "sha"]) {
                        {
                            let mut path = target_url.path_mut().unwrap();
                            path.clear();
                            path.extend_from_slice(&["github".to_owned(), "sha".to_owned(), user.to_owned(), repo.to_owned(), sha.to_string().to_owned(), method.to_owned()]);
                        }
                        set_redis_cache(&redis, &redis_key, &target_url.serialize());
                        return redir(&target_url, &req.url);

                    } else {
                        warn!("{}: SHA not found in JSON: {}", &github_url, &json);
                        return Ok(Response::with((status::NotFound,
                                                  format!("Couldn't find on Github {}", &github_url))));
                    }
                } else {
                    warn!("{}: Couldn't parse Githubs JSON response: {}",
                          &github_url,
                          &body);
                    return Ok(Response::with((status::InternalServerError,
                                              "Couldn't parse Githubs JSON response")));
                }
            } else {
                return Ok(Response::with((status::NotFound,
                                          format!("Couldn't find on Github {}", &github_url))));
            }
            Ok(Response::with(status::InternalServerError))

        }
    }
}


pub fn github_handler(req: &mut Request) -> IronResult<Response> {

    let ref router = req.extensions.get::<Router>().unwrap();
    let redis: redis::Connection = setup_redis();

    let user = router.find("user").unwrap();
    let repo = router.find("repo").unwrap();
    let sha = router.find("sha").unwrap();
    let filename: Vec<&str> = router.find("method")
                                    .unwrap_or("badge.svg")
                                    .rsplitn(2, '.')
                                    .collect();
    let (method, ext) = match filename.len() {
        2 => (filename[1], filename[0]),
        _ => (filename[0], "")
    };

    let redis_key = format!("{0}/github/{1}/{2}:{3}", method, user, repo, sha);

    match method {
        "badge" => {
            // if this is a badge, then we might have a cached version
            match redis.get(redis_key.to_owned()){
                Ok(Some(Value::Data(base_url))) => {
                    let base_url = String::from_utf8(base_url).unwrap().to_owned();
                    let target_badge = match req.url.clone().query {
                        Some(query) => format!("{}.{}?{}", base_url, ext, query),
                        _ => format!("{}.{}", base_url, ext)
                    };
                    Ok(Response::with((status::TemporaryRedirect,
                                       Redirect(iUrl::parse(&target_badge).unwrap()))))
                },
                _ => {
                    trigger_update(&user, &repo, &sha);
                    let target_badge = format!("{}.{}", LINTING_BADGE_URL, ext);
                    return redir(&Url::parse(&target_badge).unwrap(), &req.url);
                }
            }
        },
        "log" => {
            if let Ok(Some(Value::Bulk(logs))) = redis.lrange(redis_key.to_owned(), 0, -1) {
                let logs: Vec<String> = logs.iter().map(|ref v| {
                    match *v {
                        &Value::Data(ref val) => String::from_utf8(val.to_owned()).unwrap().to_owned(),
                        _ => "".to_owned()
                    }
                }).collect();
                return Ok(Response::with((status::Ok, logs.join("\n"))));
            } else {
                trigger_update(&user, &repo, &sha);
                return Ok(Response::with((status::Created,
                               "Build scheduled. Please refresh to see logs.")));
            }
        }
        "status" => {
            let redis_key = format!("result/github/{0}/{1}:{2}",
                                    user,
                                    repo,
                                    sha).to_owned();

            match redis.get(redis_key.to_owned()) {
                Ok(Some(Value::Data(status))) => Ok(Response::with((status::Ok,
                                                                    String::from_utf8(status).unwrap().to_owned()))),
                _ => {
                    trigger_update(&user, &repo, &sha);
                    Ok(Response::with((status::Created,"running")))
                }
            }
        }
        _ => {
            return Ok(Response::with((status::BadRequest,
                               format!("Not Yet Implemented: {}", method))));
        }
    }
}
