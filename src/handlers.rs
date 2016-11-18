// Handle incoming requests.
extern crate iron;
extern crate redis;
extern crate rustc_serialize;
extern crate hyper;
extern crate url;

extern crate router;

use std::vec::Vec;
use rustc_serialize::json::Json;

use iron::modifiers::Redirect;
use iron::headers::{CacheControl, CacheDirective};
use iron::prelude::*;
use iron::status;
use iron::Url as iUrl;

use hyper::client::Client;

use router::Router;

use redis::{Commands, Value};

use helpers::{setup_redis, fetch, get_status_or,  local_redir, set_redis_cache};
use github::schedule_update as schedule_github_update;

// The base URL for our badges. We aren't actually compiling them ourselves,
// but are reusing the great shields.io service.
static BADGE_URL_BASE: &'static str = "https://img.shields.io/badge/";


// Github Finder
// Expand a branch name into the hash, cache the redirect for 5min
// `/github/:user/:repo/badge.svg => /github/:user/:repo/:sha/badge.svg`
pub fn github_finder(req: &mut Request) -> IronResult<Response> {

    // Learn the parameters given to the request
    let router = req.extensions.get::<Router>().unwrap();
    let redis: redis::Connection = setup_redis();
    let hyper_client: Client = Client::new();

    let user = router.find("user").unwrap();
    let repo = router.find("repo").unwrap();
    let branch = router.find("branch").unwrap_or("master");
    let method = router.find("method").unwrap_or("badge.svg");

    // And the cache key we use to keep the map from branch->SHA
    let redis_key = format!("cached-sha/github/{0}/{1}:{2}", user, repo, branch);

    // Let's see if redis has this key. If it does, redirect the request
    // directly
    match redis.get(redis_key.to_owned()) {
        Ok(Value::Data(sha)) => {
            local_redir(&format!("/github/sha/{0}/{1}/{2}/{3}",
                                 user,
                                 repo,
                                 String::from_utf8(sha).unwrap(),
                                 method),
                        &req.url)
        }
        // otherwise, we need to look up the current SHA for the branch
        _ => {
            let github_url = format!("https://api.github.com/repos/{0}/{1}/git/refs/heads/{2}",
                                     user,
                                     repo,
                                     branch);
            // Fetch the content API request for the Github URL,
            // Parse its JSON and try to find the `SHA`-key.
            if let Some(body) = fetch(&hyper_client, &github_url) {
                if let Ok(json) = Json::from_str(&body) {
                    if let Some(&Json::String(ref sha)) = json.find_path(&["object", "sha"]) {
                        // Once found, store the SHA in the cache and redirect
                        // the request to
                        set_redis_cache(&redis, &redis_key, &sha);
                        local_redir(&format!("/github/sha/{0}/{1}/{2}/{3}",
                                             user,
                                             repo,
                                             sha,
                                             method),
                                    &req.url)
                    } else {
                        // If we couldn't find the SHA, then there is a problem
                        // we need to inform the user about. Usually this means
                        // they did a typo or the content moved – either way, we
                        // fire a 404 – Not Found.
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

// ## Github Handler
// Handle the request for a status report of a user-repo-sha combination.
// Usually the request ends up here after having been redirected via the
// `github_finder`-handler.
// In this request is where the actual sausage is done.
pub fn github_handler(req: &mut Request) -> IronResult<Response> {

    // First extract all the request information
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

    // Use `get_status_or` to look up and map the cached result
    // or trigger a `schedule_github_update` if that isn't found yet
    let result_key = format!("result/github/{0}/{1}:{2}", user, repo, sha);
    let (text, color): (String, String) = get_status_or(
        redis.get(result_key.to_owned()),
        || schedule_github_update(&user, &repo, &sha));

    // Then render the response
    let mut response = match method {
        // If this is a simple request for status, just return the result
        "status" => Response::with((status::Ok, text.to_owned())),
        // for the badge, put text, color, base URL and query-parameters from the
        // incoming requests together to the URL we need to forward it to
        "badge" => {
            let target_badge = match req.url.clone().query {
                Some(query) => format!("{}clippy-{}-{}.{}?{}", BADGE_URL_BASE, text, color, ext, query),
                _ => format!("{}clippy-{}-{}.{}", BADGE_URL_BASE, text, color, ext),
            };
            // while linting, use only temporary redirects, so that the actual
            // result will be asked for later
            Response::with((match text.as_str() {
                    "linting" => status::Found,
                    _ => status::MovedPermanently
                }, Redirect(iUrl::parse(&target_badge).unwrap())))
        },
        // emojibadge and fullemojibadge do the same as the request for `badge`,
        // except that they replace the status with appropriate emoji
        "emojibadge" => {
            let emoji = match text.as_str() {
                "linting" => "👷".to_string(),
                "failed" => "😱".to_string(),
                "success" => "👌".to_string(),
                _ => text.replace("errors", "🤕").replace("warnings", "😟")
            };

            let target_badge = match req.url.clone().query {
                Some(query) => format!("{}clippy-{}-{}.{}?{}", BADGE_URL_BASE, emoji, color, ext, query),
                _ => format!("{}clippy-{}-{}.{}", BADGE_URL_BASE, emoji, color, ext),
            };
            Response::with((match color.as_str() {
                    "blue" => status::Found,
                    _ => status::MovedPermanently
                }, Redirect(iUrl::parse(&target_badge).unwrap())))
        },
        "fullemojibadge" => {
            let emoji = match text.as_str() {
                "linting" => "👷".to_string(),
                "failed" => "😱".to_string(),
                "success" => "👌".to_string(),
                _ => text.replace("errors", "🤕").replace("warnings", "😟")
            };

            let target_badge = match req.url.clone().query {
                Some(query) => format!("{}📎-{}-{}.{}?{}", BADGE_URL_BASE, emoji, color, ext, query),
                _ => format!("{}📎-{}-{}.{}", BADGE_URL_BASE, emoji, color, ext),
            };
            Response::with((match color.as_str() {
                    "blue" => status::Found,
                    _ => status::MovedPermanently
                }, Redirect(iUrl::parse(&target_badge).unwrap())))
        },
        // If the request is asking for the logs, fetch those. This isn't particularly
        // simple as the Redis library makes the unwrapping a little bit tricky and hard
        // for rust to guess the proper types. So we have to specify the types and iterator
        // rather explictly at times.
        "log" => {
            let log_key = format!("log/github/{0}/{1}:{2}", user, repo, sha);
            match redis.lrange(log_key.to_owned(), 0, -1) {
                Ok(Some(Value::Bulk(logs))) => {
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
                    Response::with((status::Ok, logs.join("\n")))
                }
                // if there aren't any logs found, we might just started the
                // process. Let the request know.
                _ => {
                    Response::with((status::Ok, "Started. Please refresh"))
                }
            }
        },
        // Nothing else is supported – but in rust, we have to return all things
        // of the same type. So let's return a `BadRequst` :) .
        _ => Response::with((status::BadRequest, format!("{} Not Implemented.", method))),
    };

    response.headers.set(CacheControl(vec![CacheDirective::NoCache]));
    Ok(response)
}
