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

use std::fs::create_dir_all;
use std::fs::File;
use std::path::Path;
use std::io::{Read, Cursor, Result as ioResult, Write};
use std::fs;
use std::u8;
use std::env;
use std::thread;
use std::sync::mpsc;
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

use redis::{Commands, PipelineCommands};

struct ClippyResult {
    failed: bool,
    warnings: u32,
    errors: u32
}

fn update_for_github(user: &str, repo: &str, sha: &str) -> Result<ClippyResult, &'static str> {
    let redis: redis::Connection = setup_redis();
    let github_url = format!("https://github.com/{0}/{1}/archive/{2}.zip",
                             user,
                             repo,
                             sha);
    let log_key =  format!("log/github/{0}/{1}:{2}",
                             user,
                             repo,
                             sha);

    // starting a log file
    if let Ok(existing) = redis::transaction(&redis, &[log_key.clone()], |pipe| {
        match redis.exists(log_key.clone()) {
            Ok(Some(false)) => {
                pipe.cmd("RPUSH")
                        .arg(log_key.clone())
                        .arg(format!("{0} started processing github/{1}/{2}:{3}",
                                            time::now_utc().rfc3339(),
                                            user,
                                            repo,
                                            sha))
                        .ignore()
                        .execute(&redis);
                return Ok(Some(false));
                },
            _ => Ok(Some(true))
        }}) {
        if existing {
            // we have been alerted, the key already existed
            // so someone else is writing a log file. We should stop now.
            return Err("Already running!");
        }
    }

    log_redis(&redis, &log_key, "Creating Temp Directory...");

    if let Ok(tmp_dir) = TempDir::new(&format!("github_{0}_{1}_{2}",
                                                user,
                                                repo,
                                                sha)) {
        log_redis(&redis, &log_key, &format!("Fetching {}", &github_url));

        if let Some(zip_body) = fetch(&Client::new(), &github_url) {
            log_redis(&redis, &log_key, "Extracting archive");
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

                log_redis(&redis, &log_key, "Running clippy.");

                let output = Command::new("cargo").args(&["rustc", "--", "-Zunstable-options", "-Zextra-plugins=clippy", "-Zno-trans", "-lclippy", "--error-format=json"])
                                      .current_dir(&tmp_dir.path())
                                      .output()
                                      .unwrap_or_else(|e| {
                                          log_redis(&redis, &log_key,
                                                    &format!("Clippy execution failed: {}", &e));
                                          panic!("Failed to execute clippy: {}", e);
                                      });
                Err("Not Yet Implemented")
            } else {
                log_redis(&redis, &log_key, "Extracting archive failed.");
                Err("Extracting archive failed.")
            }
        } else {
            log_redis(&redis, &log_key, "Fetching from github failed!");
            Err("Fetching from github failed!")
        }
    } else {
        log_redis(&redis, &log_key, "Creating Temp Directory failed");
        Err("Creating Temp Directory failed")
    }
}

fn main() {
    // setup logger
    env_logger::init().unwrap();

    let router = router!(
        get "/github/:user/:repo/:branch/badge.svg" => github_handler,
        get "/github/:user/:repo/badge.svg" => github_handler,
        get "/" => Static::new(Path::new("static"))
    );

    warn!("Server running at 8080");
    Iron::new(router).http("0.0.0.0:8080").unwrap();

    fn github_handler(req: &mut Request) -> IronResult<Response> {

        let ref router = req.extensions.get::<Router>().unwrap();
        let redis: redis::Connection = setup_redis();

        let user = router.find("user").unwrap();
        let repo = router.find("repo").unwrap();
        let branch = router.find("branch").unwrap_or("master");
        let ext = router.find("ext").unwrap_or("svg");
        let key = format!("badge/github/{}/{}:{}", user, repo, branch);


        // Create a client.

        if let Some(url) = get_redis_redir(&redis, &key) {
            return redir(&url, &req.url);
        }

        let hyper_client: Client = Client::new();

        let github_url = format!("https://api.github.com/repos/{0}/{1}/git/refs/heads/{2}",
                                 user,
                                 repo,
                                 branch);
        if let Some(body) = fetch(&hyper_client, &github_url) {
            if let Ok(json) = Json::from_str(&body) {
                if let Some(sha) = json.find_path(&["object", "sha"]) {
                    let sha_key = format!("badge/github/{}/{}:{} ", user, repo, sha);

                    if let Some(url) = get_redis_redir(&redis, &sha_key) {
                        set_redis_cache(&redis, &key, &url.clone().serialize());
                        return redir(&url, &req.url);
                    }

                    let linting_url = format!("https://img.shields.io/badge/clippy-linting-blue.\
                                               {}?",
                                              &ext);


                    set_redis_cache(&redis, &sha_key, &linting_url);
                    set_redis_cache(&redis, &key, &linting_url);


                    let user = user.to_owned();
                    let repo =repo.to_owned();
                    let sha = sha.to_string().to_owned();
                    thread::spawn(move || {
                        update_for_github(&user, &repo, &sha);
                    });

                    // let resp = format!("There shall be content here for {}", key);
                    return redir(&Url::parse(&linting_url).unwrap(), &req.url);
                } else {
                    warn!("{}: SHA not found in JSON: {}", &github_url, &json);
                    return Ok(Response::with((status::NotFound,
                                              format!("Couldn't find on github {}", &github_url))));
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
                                      format!("Couldn't find on github {}", &github_url))));
        }


        Ok(Response::with(status::InternalServerError))
    }
}

fn log_redis(redis: &redis::Connection, key: &str, value: &str) {
    redis::pipe()
        .cmd("RPUSH")
            .arg(key.clone())
            .arg(format!("{0} {1}",
                         time::now_utc().rfc3339(),
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

fn get_redis_redir(redis: &redis::Connection, key: &str) -> Option<Url> {
    let result: Option<String> = get_redis_value(redis, key);
    if result.is_some() {
        if let Ok(url) = Url::parse(&result.unwrap()) {
            return Some(url);
        }
    }
    return None;
}


fn get_redis_value(redis: &redis::Connection, key: &str) -> Option<String> {

    if let Ok(Some(cached_result)) = redis.get(key) {
        let cached_value: Option<String> = cached_result;
        if cached_value.is_some() {
            return cached_value;
        }
    }

    return None;
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
