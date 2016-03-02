/// Run as an application, this is the starting point for our app
extern crate iron;
extern crate redis;
extern crate rustc_serialize;
extern crate hyper;
extern crate url;

extern crate router;

use std::vec::Vec;
use rustc_serialize::json::Json;

use iron::modifiers::Redirect;
use iron::prelude::*;
use iron::status;
use iron::Url as iUrl;

use hyper::client::Client;

use router::Router;

use std::slice::SliceConcatExt;
use redis::{Commands, Value};

use helpers::{setup_redis, fetch, local_redir, set_redis_cache};
use github::schedule_update as schedule_github_update;

static BADGE_URL_BASE: &'static str = "https://img.shields.io/badge/";

pub fn github_finder(req: &mut Request) -> IronResult<Response> {

    // expand a branch name into the hash, keep the redirect for 5min

    let router = req.extensions.get::<Router>().unwrap();
    let redis: redis::Connection = setup_redis();
    let hyper_client: Client = Client::new();

    let user = router.find("user").unwrap();
    let repo = router.find("repo").unwrap();
    let branch = router.find("branch").unwrap_or("master");
    let method = router.find("method").unwrap_or("badge.svg");

    let redis_key = format!("cached-sha/github/{0}/{1}:{2}", user, repo, branch);

    match redis.get(redis_key.to_owned()) {
        // we have a cached value, redirect directly
        Ok(Value::Data(sha)) => {
            local_redir(&format!("/github/sha/{0}/{1}/{2}/{3}",
                                 user,
                                 repo,
                                 String::from_utf8(sha).unwrap(),
                                 method),
                        &req.url)
        }
        _ => {
            let github_url = format!("https://api.github.com/repos/{0}/{1}/git/refs/heads/{2}",
                                     user,
                                     repo,
                                     branch);
            if let Some(body) = fetch(&hyper_client, &github_url) {
                if let Ok(json) = Json::from_str(&body) {
                    if let Some(&Json::String(ref sha)) = json.find_path(&["object", "sha"]) {
                        set_redis_cache(&redis, &redis_key, &sha);
                        local_redir(&format!("/github/sha/{0}/{1}/{2}/{3}",
                                             user,
                                             repo,
                                             sha,
                                             method),
                                    &req.url)
                    } else {
                        warn!("{}: SHA not found in JSON: {}", &github_url, &json);
                        Ok(Response::with((status::NotFound,
                                           format!("Couldn't find on Github {}", &github_url))))
                    }
                } else {
                    warn!("{}: Couldn't parse Githubs JSON response: {}",
                          &github_url,
                          &body);
                    Ok(Response::with((status::InternalServerError,
                                       "Couldn't parse Githubs JSON response")))
                }
            } else {
                Ok(Response::with((status::NotFound,
                                   format!("Couldn't find on Github {}", &github_url))))
            }
        }
    }
}


pub fn github_handler(req: &mut Request) -> IronResult<Response> {

    let router = req.extensions.get::<Router>().unwrap();
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
        _ => (filename[0], ""),
    };

    let redis_key = format!("{0}/github/{1}/{2}:{3}", method, user, repo, sha);

    match method {
        "badge" => {
            // if this is a badge, then we might have a cached version
            let redis_key = format!("result/github/{0}/{1}:{2}", user, repo, sha).to_owned();
            let url = match redis.get(redis_key.to_owned()) {
                Ok(Some(Value::Data(status))) => {
                    let status = String::from_utf8(status).unwrap().to_owned();
                    format!("{}clippy-{}-{}", BADGE_URL_BASE, status, match status.as_str() {
                        "success" => "brightgreen",
                        "failed" => "red",
                        "linting" => "blue",
                        _ => {
                            if status.ends_with("errors") {
                                "red"
                            } else { // warnings
                                "yellow"
                            }
                        }
                    })
                }
                _ => {
                    schedule_github_update(&user, &repo, &sha);
                    format!("{}clippy-linting-blue", BADGE_URL_BASE)
                }
            };

            let target_badge = match req.url.clone().query {
                Some(query) => format!("{}.{}?{}", url, ext, query),
                _ => format!("{}.{}", url, ext),
            };
            Ok(Response::with((status::TemporaryRedirect,
                               Redirect(iUrl::parse(&target_badge).unwrap()))))
        },
        "log" => {
            match redis.lrange(redis_key.to_owned(), 0, -1) {
                Ok(Some(Value::Bulk(logs))) => {
                    match logs.len() {
                        0 => {
                            schedule_github_update(&user, &repo, &sha);
                            Ok(Response::with((status::Ok, "Started. Please refresh")))
                        }
                        _ => {
                            let logs: Vec<String> = logs.iter()
                                                        .map(|ref v| {
                                                            match **v {
                                                                Value::Data(ref val) => {
                                                String::from_utf8(val.to_owned())
                                                    .unwrap()
                                                    .to_owned()
                                            }
                                                                _ => "".to_owned(),
                                                            }
                                                        })
                                                        .collect();
                            Ok(Response::with((status::Ok, logs.join("\n"))))
                        }
                    }
                }
                _ => {
                    schedule_github_update(&user, &repo, &sha);
                    Ok(Response::with((status::Ok, "Started. Please refresh")))
                }
            }
        }
        "status" => {
            let redis_key = format!("result/github/{0}/{1}:{2}", user, repo, sha).to_owned();

            match redis.get(redis_key.to_owned()) {
                Ok(Some(Value::Data(status))) => {
                    Ok(Response::with((status::Ok, String::from_utf8(status).unwrap().to_owned())))
                }
                _ => {
                    schedule_github_update(&user, &repo, &sha);
                    Ok(Response::with((status::Ok, "linting")))
                }
            }
        }
        _ => Ok(Response::with((status::BadRequest, format!("{} Not Implemented.", method)))),
    }
}
