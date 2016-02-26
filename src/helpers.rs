/// Run as an application, this is the starting point for our app
extern crate iron;
extern crate redis;
extern crate hyper;
extern crate url;
extern crate time;

extern crate router;
extern crate mime;

// for logging
extern crate log;
extern crate env_logger;

use std::io::Read;
use std::env;
use time::now_utc;

use iron::modifiers::Redirect;
use iron::prelude::*;
use iron::status;
use iron::Url as iUrl;

use url::Url;

use hyper::client::Client;
use hyper::header::qitem;
use hyper::header;

use redis::{Commands, PipelineCommands};

pub fn log_redis(redis: &redis::Connection, key: &str, value: &str) {
    redis::pipe()
        .cmd("RPUSH")
            .arg(key.clone())
            .arg(format!("{0} {1}",
                         now_utc().rfc3339(),
                         value))
            .ignore()
        .execute(redis);
}

pub fn set_redis_cache(redis: &redis::Connection, key: &str, value: &str) {
    redis::pipe()
        .cmd("SET").arg(key.clone()).arg(value).ignore()
        .cmd("EXPIRE").arg(key.clone()).arg(5 * 60).ignore() // we expire in 5min
        .execute(redis);
}

pub fn redir(url: &Url, source_url: &iUrl) -> IronResult<Response> {
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

pub fn fetch(client: &Client, url: &str) -> Option<String> {
    let res = client.get(url)
                    .header(header::UserAgent("Clippy/0.1".to_owned()))
                    .header(header::Accept(vec![qitem(mime!(_/_))]))
                    .header(header::Connection::close());
    if let Ok(mut res) = res.send() {
        let mut body = String::new();
        if res.read_to_string(&mut body).is_ok() {
            return Some(body);
        }
    }
    return None;
}

pub fn setup_redis() -> redis::Connection {
    let url = redis::parse_redis_url(&env::var("REDIS_URL")
                                          .unwrap_or("redis://redis/".to_owned()))
                  .unwrap();
    redis::Client::open(url)
        .unwrap()
        .get_connection()
        .unwrap()
}
