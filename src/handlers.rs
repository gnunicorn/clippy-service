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

use url::Url;

use hyper::client::Client;

use router::Router;

use std::slice::SliceConcatExt;
use redis::{Commands, Value};

use helpers::{setup_redis, fetch, redir, set_redis_cache};
use github::{schedule_update as schedule_github_update};

static LINTING_BADGE_URL: &'static str = "https://img.shields.io/badge/clippy-linting-blue";

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
    let mut target_url = req.url.clone().into_generic_url().to_owned();

    match redis.get(redis_key.to_owned()){
        // we have a cached value, redirect directly
        Ok(Value::Data(sha)) =>{
            {
                let mut path = target_url.path_mut().unwrap();
                path.clear();
                path.extend_from_slice(&["github".to_owned(), "sha".to_owned(), user.to_owned(), repo.to_owned(), String::from_utf8(sha).unwrap().to_owned(), method.to_owned()]);
            }
            redir(&target_url, &req.url)
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
                            path.extend_from_slice(&["github".to_owned(), "sha".to_owned(), user.to_owned(), repo.to_owned(), sha.clone().to_owned(), method.to_owned()]);
                        }
                        set_redis_cache(&redis, &redis_key, &target_url.serialize());
                        redir(&target_url, &req.url)
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
                    schedule_github_update(&user, &repo, &sha);
                    let target_badge = format!("{}.{}", LINTING_BADGE_URL, ext);
                    redir(&Url::parse(&target_badge).unwrap(), &req.url)
                }
            }
        },
        "log" => match redis.lrange(redis_key.to_owned(), 0, -1) {
            Ok(Some(Value::Bulk(logs))) => match logs.len() {
                0 => {
                    schedule_github_update(&user, &repo, &sha);
                    Ok(Response::with((status::Ok, "Started. Please refresh")))
                }
                _ => {
                    let logs: Vec<String> = logs.iter().map(|ref v| {
                        match **v {
                            Value::Data(ref val) => String::from_utf8(val.to_owned()).unwrap().to_owned(),
                            _ => "".to_owned()
                        }
                    }).collect();
                    Ok(Response::with((status::Ok, logs.join("\n"))))
                }
            },
            _ => {
                schedule_github_update(&user, &repo, &sha);
                Ok(Response::with((status::Ok, "Started. Please refresh")))
            }
        },
        "status" => {
            let redis_key = format!("result/github/{0}/{1}:{2}",
                                    user,
                                    repo,
                                    sha).to_owned();

            match redis.get(redis_key.to_owned()) {
                Ok(Some(Value::Data(status))) => Ok(Response::with((status::Ok,
                                                                    String::from_utf8(status).unwrap().to_owned()))),
                _ => {
                    schedule_github_update(&user, &repo, &sha);
                    Ok(Response::with((status::Ok, "linting")))
                }
            }
        }
        _ => Ok(Response::with((status::BadRequest,
                                format!("{} Not Implemented.", method))))
    }
}
