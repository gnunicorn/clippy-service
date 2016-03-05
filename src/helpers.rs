// A bunch of helper functions reused everywhere in the project
// to remove boilerplate code

// We start as usual by defining the external crates we want to use
extern crate iron;
extern crate redis;
extern crate hyper;
extern crate url;
extern crate time;

extern crate router;
extern crate mime;
extern crate tempdir;
extern crate zip;

// and the specific imports we want
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

use iron::headers::{Location, CacheControl, CacheDirective};
use iron::prelude::*;
use iron::status;
use iron::Url as iUrl;

use hyper::client::Client;
use hyper::header::qitem;
use hyper::header;

// ## Download And Unzip
// Given `source_url` and a target directory `tmp_dir` this helper function
// tries to do download and unzip the given file there. Or returns a String with
// the error message of what went wrong trying.
pub fn download_and_unzip(source_url: &str, tmp_dir: &TempDir) -> Result<Vec<String>, String> {

    // Start by creating a hyper client, which tries to connect and requests
    // the content of that url. Specifically for the github services, make sure
    // to have defined the `UserAgent`- and `Accept` HTTP-Header.
    //
    // The [Hyper Accept Header](http://ironframework.io/doc/hyper/header/struct.Accept.html)
    // expects a vector of [QualityItem](http://ironframework.io/doc/hyper/header/struct.QualityItem.html),
    // which in turn should contain the proper [Mime](http://ironframework.io/doc/hyper/header/struct.QualityItem.html)Type.
    // In our case, we just want it to accept everything (`*/*` in HTTP-Speak), which
    // translate in the usage of the handy [`mime!`](http://ironframework.io/doc/mime/macro.mime!.html)
    // -macro with `_/_` as the parameter.
    let client = Client::new();
    let res = client.get(&source_url.to_owned())
                    .header(header::UserAgent("Clippy/1.0".to_owned()))
                    .header(header::Accept(vec![qitem(mime!(_/_))]))
                    .header(header::Connection::close());

    // once we are done preparing, let's send the request
    match res.send() {
        // if we have a connection, we will try read the body
        // into a buffer, a `u8`-Vector.
        Ok(mut res) => {
            let mut zip_body: Vec<u8> = Vec::new();
            match res.read_to_end(&mut zip_body) {
                // if that succeeded, we pass this vector, wrapped into a Cursor
                // (as ZipArchive requires readable trait) to ZipArchive for
                // unzipping and processing.
                Ok(_) => {
                    match ZipArchive::new(Cursor::new(zip_body)) {
                        // if ZipArchive was able to read the metadata,
                        // it is time to unzip its contents
                        Ok(mut archive) => {
                            let mut paths: Vec<String> = Vec::new();
                            // for every file, ZipArchive identified in the response,
                            // we try to unpack it into the specified `tmp_dir`
                            for i in 0..archive.len() {
                                let mut zip_file = archive.by_index(i).unwrap();
                                let extracted_path = tmp_dir.path().join(zip_file.name());
                                let full_path = extracted_path.as_path();

                                // Zip uses the size of `0` to inform us that something
                                // is actually a directory. In that case, we don't try to
                                // read the content but instead set up the directory
                                // structure for it: `create_dir_all` recursively creates
                                // the directory path if not existing.
                                if zip_file.size() == 0 {
                                    fs::create_dir_all(full_path).unwrap();
                                } else {
                                    // for any other size, we have a proper file.
                                    // read the uncompressed content into a buffer
                                    // and write that into the specified target file
                                    let mut writer = File::create(full_path).unwrap();
                                    let mut buffer: Vec<u8> = vec![];
                                    zip_file.read_to_end(&mut buffer).unwrap();
                                    writer.write(&buffer).unwrap();
                                    // lastly, add the file path to the vectors of
                                    // paths to give back
                                    paths.push(String::from(full_path.to_string_lossy()
                                                                     .into_owned()));
                                }
                            }
                            // all went fine, all files extracted, return with `Ok`
                            // and the list of those paths
                            Ok(paths)
                        }
                        // Unfortunately we ran into a ZipArchive Error
                        Err(zip::result::ZipError::InvalidArchive(error)) |
                        Err(zip::result::ZipError::UnsupportedArchive(error)) => {
                            Err(format!("Extracting archive failed: {}", error).to_owned())
                        }
                        // ZipArchive told us about a file, which doesn't exist,
                        // this should really never happen, as we use references
                        // given by it. The only plausible cause for this is a corrupt
                        // Zip Archive – so state that.
                        Err(zip::result::ZipError::FileNotFound) => {
                            Err(String::from("Zip Archive Corrupt"))
                        }
                        Err(_) => Err(String::from("General IO Error")),
                    }
                }
                // Github did respond with something, Zip couldn't understand
                // – often a 404 or error on githubs side. Bubble this error up in
                // the wrapped string for the requester to debug.
                Err(error) => Err(format!("Couldn't read github response: {}", error)),
            }
        }
        // We weren't able to connect to github. Let them know what happened.
        Err(error) => Err(format!("Couldn't connect to github: {}", error)),
    }
    // *Note*: While the `match () => { Ok(x) => ..., Err(x) => ...}` is a little
    // tedious to write (and ugly to read), Rust enforces you to be incredibly specific
    // with your error handling that way. While this might be a little in the way for
    // fast prototyping, it forces you to write those specific error messages, you always
    // wished this stupid API provided you with.
}


// ## Setup Redis
// Redis is the database backend we use for almost everything. This function
// looks up the configured REDIS_URL (from the environment) and returns a
// `redis::Connection` ready to be used.
pub fn setup_redis() -> redis::Connection {
    // Read the environment Variable "REDIS_URL" or fallback to "redis://localhost/"
    // if not found. This variable is the default used by Dokku for the external
    // database we are connected to.
    let url = redis::parse_redis_url(&env::var("REDIS_URL").unwrap_or("redis://localhost/".to_owned()))
                  .unwrap();
    redis::Client::open(url)
        .unwrap()
        .get_connection()
        .unwrap()
}


// ## Log Redis
// We use Redis to store a public log of what happened during processing. This is a
// handy function which, given the redis connection, the log-key and the log statement
// appends it to the redis log list including the current timestamp.
pub fn log_redis(redis: &redis::Connection, key: &str, value: &str) {
    redis::pipe()
        .cmd("RPUSH")
        .arg(key.clone())
        .arg(format!("{0} {1}", now_utc().rfc3339(), value))
        .ignore()
        .execute(redis);
}


// ## Set Redis Cache
// We use Redis for caching. This handy function sets the value and expires it.
pub fn set_redis_cache(redis: &redis::Connection, key: &str, value: &str) {
    redis::pipe()
        .cmd("SET").arg(key.clone()).arg(value).ignore()
        .cmd("EXPIRE").arg(key.clone()).arg(5 * 60).ignore() // we expire in 5min
        .execute(redis);
}


// ## Get Status Or
// Reads the result of a Redis-Get-Query for the cached result and unpacks the value into
// the badge relevant information of "text" and badge color we want to use, OR calls
// the passed in `trigger` function if parsing failed. This is a handy function to look
// up the redis key result, parse it or start the background process of executing a
// clippy update
pub fn get_status_or<F>(result: RedisResult<Option<Value>>, trigger: F) -> (String, String)
    where F: Fn() {
    match result {
        // Redis wraps the content in deep packs
        // With this comprehensive check of `Value` of type `Data` in `Some` in `Ok` we
        // can be fairly certain this is the status we had stored before.
        // Unfortunately that means, we get the raw Vector of `u8` here in `status` so
        // we need to wrap that ourselfes again, before we can process
        Ok(Some(Value::Data(status))) => {
            let status = String::from_utf8(status).unwrap().to_owned();
            (status.clone(), String::from(match status.as_str() {
                // Map the status code to the appropriate color
                "success" => "brightgreen",
                "failed" => "red",
                "linting" => "blue",
                _ => {
                    // Warnings and Errors contain the count, so we can't
                    // directly map them.
                    if status.ends_with("errors") {
                        "red"
                    } else { // warnings
                        "yellow"
                    }
                }
            }))
        }
        _ => {
            // The result given isn't a proper status as we expect it to
            // be stored. Trigger the update and return that we are "linting"
            trigger();
            (String::from("linting"), String::from("blue"))
        }
    }
}

// ## local_redir
// This handy function builds a IronResult with the Location-header redirecting
// us to an abitraty string. This is needed because iron requires us to pass
// a URL object otherwise, which we can't use for relative redirects, which is
// necessary to redirect the branch to the proper SHA-key. Also set a cache
// control header to do this redirect for 300seconds only
pub fn local_redir(url: &str, source_url: &iUrl) -> IronResult<Response> {
    let mut resp = Response::with(status::Found);
    resp.headers.set(CacheControl(vec![CacheDirective::MaxAge(300)]));
    // As a special feature, this redirect also copies any query-parameters
    // coming in to the parameter redirected to.
    match source_url.query {
        Some(ref query) => resp.headers.set(Location(format!("{}?{}", &url, query))),
        _ => resp.headers.set(Location(url.to_owned())),
    }
    Ok(resp)
}

// ## fetch
// Fetches a HTTP URL and returns the content as a String or `None` if anything
// went wrong. Used as a handy function because Response reading is a little
// too verbose sometimes.
pub fn fetch(client: &Client, url: &str) -> Option<String> {
    let res = client.get(url)
                    .header(header::UserAgent("Clippy/1.0".to_owned()))
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
