 #![feature(slice_concat_ext)]

extern crate iron;
extern crate staticfile;
extern crate redis;
extern crate rustc_serialize;
extern crate hyper;
extern crate url;
extern crate time;
extern crate tempdir;
extern crate zip;

#[macro_use]
extern crate router;

// for logging
#[macro_use]
extern crate log;
extern crate env_logger;

use std::fs::File;
use std::path::Path;
use std::io::{Read, Cursor, Write};
use std::fs;
use std::vec::Vec;
use std::env;
use std::thread;
use std::process::Command;
use tempdir::TempDir;
use time::now_utc;
use zip::*;

use rustc_serialize::json::Json;

use iron::modifiers::Redirect;
use iron::prelude::*;
use iron::status;
use iron::Url as iUrl;

use url::Url;

use hyper::Client;
use hyper::header;

use staticfile::Static;
use router::Router;

use std::slice::SliceConcatExt;
use redis::{Commands, PipelineCommands, Value};

pub enum ClippyState {
    Running,
    EndedFine,
    EndedWithWarnings,
    EndedWithErrors
}

struct ClippyResult {
    state: ClippyState,
    warnings: i32,
    errors: i32,
}

static LINTING_BADGE_URL: &'static str = "https://img.shields.io/badge/clippy-linting-blue";

fn update_for_github(redis: &redis::Connection, user: &str, repo: &str, sha: &str) -> Result<ClippyResult, &'static str> {
    let github_url = format!("https://github.com/{0}/{1}/archive/{2}.zip",
                             user,
                             repo,
                             sha);
    let base_key = format!("github/{0}/{1}:{2}",
                             user,
                             repo,
                             sha);
    let log_key =  format!("log/{}", &base_key);
    let badge_key =  format!("badge/{}", &base_key);
    // let result_key =  format!("result/{}", &base_key);

    // starting a log file
    if let Ok(existing) = redis::transaction(redis, &[log_key.clone(), badge_key.clone()], |pipe| {
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
                    .execute(redis);
                return Ok(Some(false));
                },
            _ => Ok(Some(true))
        }}) {
        if existing {
            // we have been alerted, the key already existed
            // so someone else is writing a log file. We should stop now.
            return Ok(ClippyResult{state: ClippyState::Running, warnings: 0, errors: 0})
        }
    }

    log_redis(redis, &log_key, "Creating Temp Directory...");

    if let Ok(tmp_dir) = TempDir::new(&format!("github_{0}_{1}_{2}",
                                                user,
                                                repo,
                                                sha)) {
        log_redis(redis, &log_key, &format!("Fetching {}", &github_url));

        if let Some(zip_body) = fetch(&Client::new(), &github_url) {
            log_redis(redis, &log_key, "Extracting archive");
            if let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(zip_body)) {
                for i in 0..archive.len()
                {
                    let mut zip_file = archive.by_index(i).unwrap();
                    let full_path = tmp_dir.path().join(zip_file.name());
                    let extracted_path = full_path.as_path();
                    let mut writer = File::create(extracted_path).unwrap();
                    let mut buffer: Vec<u8> = vec![];

                    fs::create_dir_all(extracted_path.parent().unwrap()).unwrap();
                    zip_file.read_to_end(&mut buffer).unwrap();
                    writer.write(&buffer).unwrap();
                }

                log_redis(redis, &log_key, "Running clippy.");

                let output = Command::new("cargo").args(&["rustc", "--", "-Zunstable-options", "-Zextra-plugins=clippy", "-Zno-trans", "-lclippy", "--error-format=json"])
                                      .current_dir(&tmp_dir.path())
                                      .output()
                                      .unwrap_or_else(|e| {
                                          log_redis(redis, &log_key,
                                                    &format!("Clippy execution failed: {}", &e));
                                          panic!("Failed to execute clippy: {}", e);
                                      });
                Err("Not Yet Implemented")
            } else {
                log_redis(redis, &log_key, "Extracting archive failed.");
                Err("Extracting archive failed.")
            }
        } else {
            log_redis(redis, &log_key, "Fetching from github failed!");
            Err("Fetching from github failed!")
        }
    } else {
        log_redis(redis, &log_key, "Creating Temp Directory failed");
        Err("Creating Temp Directory failed")
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
    let badge_key = format!("badge/{}", base_key).to_owned();

    thread::spawn(move || {
        let redis: redis::Connection = setup_redis();
        let (text, color) : (String, &str) = match update_for_github(&redis, &user, &repo, &sha) {
            Ok(result) => {
                match result.state {
                    ClippyState::Running => (String::from("linting"), "blue"),
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
            _ => (String::from("failed"), "red")
        };
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

fn main() {
    // setup logger
    env_logger::init().unwrap();

    let router = router!(
        get "/github/sha/:user/:repo/:sha/:method" => github_handler,
        get "/github/:user/:repo/:branch/:method" => github_finder,
        get "/github/:user/:repo/:method" => github_finder,
        get "/" => Static::new(Path::new("static"))
    );

    warn!("Server running at 8080");
    Iron::new(router).http("0.0.0.0:8080").unwrap();

    fn github_finder(req: &mut Request) -> IronResult<Response> {

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


    fn github_handler(req: &mut Request) -> IronResult<Response> {

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
                    // let logs: Vec<Value::Data> = logs;
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
}

fn log_redis(redis: &redis::Connection, key: &str, value: &str) {
    redis::pipe()
        .cmd("RPUSH")
            .arg(key.clone())
            .arg(format!("{0} {1}",
                         now_utc().rfc3339(),
                         value))
            .ignore()
        .execute(redis);
}

fn set_redis_cache(redis: &redis::Connection, key: &str, value: &str) {
    redis::pipe()
        .cmd("SET").arg(key.clone()).arg(value).ignore()
        .cmd("EXPIRE").arg(key.clone()).arg(5 * 60).ignore() // we expire in 5min
        .execute(redis);
}

fn redir(url: &Url, source_url: &iUrl) -> IronResult<Response> {
    match iUrl::from_generic_url(url.clone()) {
        Ok(mut redir_url) => {
            if let Some(ref query) = source_url.query {
                redir_url.query = Some(query.clone());
            }
            Ok(Response::with((status::TemporaryRedirect, Redirect(redir_url))))
        }
        Err(err) => Ok(Response::with((status::InternalServerError, err))),
    }
}

fn fetch(client: &Client, url: &str) -> Option<String> {
    let res = client.get(url)
                    .header(header::UserAgent("Clippy".to_owned()))
                    .header(header::Connection::close());
    if let Ok(mut res) = res.send() {
        let mut body = String::new();
        if res.read_to_string(&mut body).is_ok() {
            return Some(body);
        }
    }

    return None;
}

fn setup_redis() -> redis::Connection {
    let url = redis::parse_redis_url(&env::var("REDIS_URL")
                                          .unwrap_or("redis://redis/".to_owned()))
                  .unwrap();
    redis::Client::open(url)
        .unwrap()
        .get_connection()
        .unwrap()
}
