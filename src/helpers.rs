/// Run as an application, this is the starting point for our app
extern crate iron;
extern crate redis;
extern crate hyper;
extern crate url;
extern crate time;

extern crate router;
extern crate mime;
extern crate tempdir;
extern crate zip;

use std::fs::File;
use std::io::{Read, Cursor, Write};
use std::fs;
use std::vec::Vec;
use std::env;
use tempdir::TempDir;
use time::now_utc;
use zip::ZipArchive;

use std::slice::SliceConcatExt;
use redis::{Commands, RedisResult, PipelineCommands, Value};

use iron::headers::Location;
use iron::prelude::*;
use iron::status;
use iron::Url as iUrl;

use hyper::client::Client;
use hyper::header::qitem;
use hyper::header;


pub fn download_and_unzip(source_url: &str, tmp_dir: &TempDir) -> Result<Vec<String>, String> {

    let client = Client::new();
    let res = client.get(&source_url.to_owned())
                    .header(header::UserAgent("Clippy/0.9.0".to_owned()))
                    .header(header::Accept(vec![qitem(mime!(_/_))]))
                    .header(header::Connection::close());

    match res.send() {
        Ok(mut res) => {
            let mut zip_body: Vec<u8> = Vec::new();
            match res.read_to_end(&mut zip_body) {
                Ok(_) => {
                    match ZipArchive::new(Cursor::new(zip_body)) {
                        Ok(mut archive) => {
                            let mut paths: Vec<String> = Vec::new();
                            for i in 0..archive.len() {
                                let mut zip_file = archive.by_index(i).unwrap();
                                let extracted_path = tmp_dir.path().join(zip_file.name());
                                let full_path = extracted_path.as_path();

                                if zip_file.size() == 0 {
                                    fs::create_dir_all(full_path).unwrap();
                                } else {
                                    let mut writer = File::create(full_path).unwrap();
                                    let mut buffer: Vec<u8> = vec![];
                                    zip_file.read_to_end(&mut buffer).unwrap();
                                    writer.write(&buffer).unwrap();
                                    paths.push(String::from(full_path.to_string_lossy()
                                                                     .into_owned()));
                                }
                            }
                            Ok(paths)
                        }
                        Err(zip::result::ZipError::InvalidArchive(error)) |
                        Err(zip::result::ZipError::UnsupportedArchive(error)) => {
                            Err(format!("Extracting archive failed: {}", error).to_owned())
                        }
                        Err(zip::result::ZipError::FileNotFound) => {
                            Err(String::from("Zip  Archive Corrupt"))
                        }
                        Err(_) => Err(String::from("General IO Error")),
                    }
                }
                Err(error) => Err(format!("Couldn't read github response: {}", error)),
            }
        }
        Err(error) => Err(format!("Couldn't connect to github: {}", error)),
    }
}

pub fn log_redis(redis: &redis::Connection, key: &str, value: &str) {
    redis::pipe()
        .cmd("RPUSH")
        .arg(key.clone())
        .arg(format!("{0} {1}", now_utc().rfc3339(), value))
        .ignore()
        .execute(redis);
}

pub fn set_redis_cache(redis: &redis::Connection, key: &str, value: &str) {
    redis::pipe()
        .cmd("SET").arg(key.clone()).arg(value).ignore()
        .cmd("EXPIRE").arg(key.clone()).arg(5 * 60).ignore() // we expire in 5min
        .execute(redis);
}


pub fn get_status_or<F>(result: RedisResult<Option<Value>>, trigger: F) -> (String, String)
    where F: Fn() {
    match result {
        Ok(Some(Value::Data(status))) => {
            let status = String::from_utf8(status).unwrap().to_owned();
            (status.clone(), String::from(match status.as_str() {
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
            }))
        }
        _ => {
            trigger();
            (String::from("linting"), String::from("blue"))
        }
    }
}

pub fn local_redir(url: &str, source_url: &iUrl) -> IronResult<Response> {
    let mut resp = Response::with(status::TemporaryRedirect);
    match source_url.query {
        Some(ref query) => resp.headers.set(Location(format!("{}?{}", &url, query))),
        _ => resp.headers.set(Location(url.to_owned())),
    }
    Ok(resp)
}

pub fn fetch(client: &Client, url: &str) -> Option<String> {
    let res = client.get(url)
                    .header(header::UserAgent("Clippy/0.9.0".to_owned()))
                    .header(header::Accept(vec![qitem(mime!(_/_))]))
                    .header(header::Connection::close());
    if let Ok(mut res) = res.send() {
        let mut body = String::new();
        if res.read_to_string(&mut body).is_ok() {
            return Some(body);
        }
    }
    None
}

pub fn setup_redis() -> redis::Connection {
    let url = redis::parse_redis_url(&env::var("REDIS_URL").unwrap_or("redis://localhost/".to_owned()))
                  .unwrap();
    redis::Client::open(url)
        .unwrap()
        .get_connection()
        .unwrap()
}
