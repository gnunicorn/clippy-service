extern crate iron;
#[macro_use]
extern crate router;
extern crate staticfile;


use std::path::Path;

use iron::{Iron, Request, Response, IronResult};
use staticfile::Static;

fn main() {
    let router = router!(
        get "/github/:repo/:user/:branch.png" => github_handler,
        get "/" => Static::new(Path::new("static"))
    );

    Iron::new(router).http("localhost:8080").unwrap();
    println!("Check out 8080");

    fn github_handler(_: &mut Request) -> IronResult<Response> {
        Ok(Response::with((iron::status::Ok, "Hello World")))
    }
}
