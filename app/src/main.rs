extern crate iron;
#[macro_use]
extern crate router;
extern crate staticfile;


use std::path::Path;

use iron::prelude::*;
use staticfile::Static;
use router::Router;
use iron::status;

fn main() {
    let router = router!(
        get "/github/:user/:repo/:branch/badge.png" => github_handler,
        get "/github/:user/:repo/badge.png" => github_handler,
        get "/" => Static::new(Path::new("static"))
    );

    Iron::new(router).http("0.0.0.0:8080").unwrap();
    println!("Check out 8080");

    fn github_handler(req: &mut Request) -> IronResult<Response> {

        let ref router = req.extensions.get::<Router>().unwrap();
        let user = router.find("user").unwrap();
        let repo = router.find("repo").unwrap();
        let branch = router.find("branch").unwrap_or("master");
        let resp = format!("There shall be content here for {}/{}:{} ", user, repo, branch);
        Ok(Response::with((status::Ok, resp)))
    }
}
