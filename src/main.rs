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

use std::fs::File;
use std::path::Path;
use std::io::{Read, Cursor, Write};
use std::fs;
use std::u8;
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

use hyper::client::{Client, RedirectPolicy};
use hyper::header::{Headers, Accept, qitem};
use hyper::mime::{Mime, TopLevel, SubLevel};
use hyper::header;

use staticfile::Static;
use router::Router;

use std::slice::SliceConcatExt;
use redis::{Commands, PipelineCommands, Value};

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
