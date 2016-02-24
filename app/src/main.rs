extern crate iron;
extern crate staticfile;
extern crate redis;
extern crate rustc_serialize;
extern crate hyper;
extern crate url;

#[macro_use]
extern crate router;


// for logging
#[macro_use]
extern crate log;
extern crate env_logger;

use std::path::Path;
use std::io::Read;
use std::env;

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

use redis::Commands;

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
        let user = router.find("user").unwrap();
        let repo = router.find("repo").unwrap();
        let branch = router.find("branch").unwrap_or("master");
        let ext = router.find("ext").unwrap_or("svg");
        let key = format!("{}/{}:{} ", user, repo, branch);

        let redis: redis::Connection = setup_redis();
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
                    let sha_key = format!("{}/{}:{} ", user, repo, sha);

                    if let Some(url) = get_redis_redir(&redis, &sha_key) {
                        set_redis_cache(&redis, &key, &url.clone().serialize());
                        return redir(&url, &req.url);
                    }

                    let linting_url = format!("https://img.shields.io/badge/clippy-linting-blue.\
                                               {}?",
                                              &ext);


                    set_redis_cache(&redis, &sha_key, &linting_url);
                    set_redis_cache(&redis, &key, &linting_url);

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

    let cached_result = redis.get(key);

    if cached_result.is_ok() {
        let cached_value: Option<String> = cached_result.unwrap();
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
                                          .unwrap_or("redis://redis/".to_string()))
                  .unwrap();
    redis::Client::open(url)
        .unwrap()
        .get_connection()
        .unwrap()
}
