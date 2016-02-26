#![feature(slice_concat_ext)]

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

#[macro_use]
extern crate router;

#[macro_use]
extern crate mime;

// for logging
#[macro_use]
extern crate log;
extern crate env_logger;

use std::path::Path;
use iron::prelude::*;
use staticfile::Static;

mod handlers;

fn main() {
    // setup logger
    env_logger::init().unwrap();

    let router = router!(
        get "/github/sha/:user/:repo/:sha/:method" => handlers::github_handler,
        get "/github/:user/:repo/:branch/:method" => handlers::github_finder,
        get "/github/:user/:repo/:method" => handlers::github_finder,
        get "/" => Static::new(Path::new("static"))
    );

    warn!("Server running at 8080");
    Iron::new(router).http("0.0.0.0:8080").unwrap();
}
