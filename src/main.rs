// **Run as an application, this is where execution starts.**

// Further more we define our app dependencies on external crates.
// First everything that is directly related to the iron framework
// are using to build our web project on
extern crate iron;
extern crate staticfile;
extern crate mount;

// we will use the (iron) router macro
#[macro_use]
extern crate router;

// Secondly all high level libraries, like serializers, hyper and redis
extern crate rustc_serialize;
extern crate hyper;
extern crate redis;

// And last all the very common utils, like loggers, tempdir, time
// We want to use the log macros globally
#[macro_use]
extern crate log;
extern crate env_logger;

// the macro to define Mime types
#[macro_use]
extern crate mime;

extern crate tempdir;
extern crate url;
extern crate time;
extern crate zip;

// Next, we need to _register_ the other modules of this crate here, using the `mod` keyword:
// We want it to use the handlers, helpers, github and clippy modules (all in their
// respective files)

mod handlers;
mod helpers;
mod github;
mod clippy;

// Then we  _import_ the things specifically needed for this particular module
// again starting with iron, its specifics and lastly common libs
use iron::prelude::*;
use staticfile::Static;
use mount::Mount;

use std::path::Path;


// **The `main` function** in `src/main.rs` is the entry point for our command when it will
// be executed. In our case, this is where we setup the logger, routing and start the
// iron server.
fn main() {
    // setup logger using [env_logger](http://doc.rust-lang.org/log/env_logger/index.html) crate.
    // Thus, you can specify the log output with the handy `RUST_LOG` environment variable
    env_logger::init().unwrap();

    // In order to react to incoming requests, we set up a multiple mount points, based
    // on the first part of the url.
    let mut mount = Mount::new();

    // Everything starting with `/github/` should be routed to our github handlers
    // in `handler.rs`.
    // We are using the [`router!`-macro](http://ironframework.io/doc/router/macro.router!.html)
    // here because it offers a much more readable way of specifying the routing
    // table:
    // ```
    //   METHOD "URL/:with_keywords" => HANDLER
    // ```
    mount.mount("/github/", router!(
        get "/sha/:user/:repo/:sha/:method" => handlers::github_handler,
        get "/:user/:repo/:branch/:method" => handlers::github_finder,
        get "/:user/:repo/:method" => handlers::github_finder
    ));

    // Secondly we have some static files in the static/ folder we'd like to have served.
    // *Note*: We have to define them seperately as Static _does not_ serve recursively
    // at the time of writing.
    mount.mount("/docs/public/fonts/", Static::new(Path::new("static/docs/public/fonts/")));
    mount.mount("/docs/", Static::new(Path::new("static/docs/")));
    mount.mount("/", Static::new(Path::new("static")));

    // Send a message to the console, letting the user know we are (going to be) up
    warn!("Server running at 5000");

    // And start serving those routes
    // On port `5000` of all interfaces
    Iron::new(mount).http("0.0.0.0:5000").unwrap();
}
