#![feature(plugin)]
#![plugin(maud_macros)]

extern crate iron;
extern crate bodyparser;
extern crate time;
extern crate maud;
extern crate rand;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate hyper;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::env;
use iron::prelude::*;
use iron::{Handler};
use iron::{status, method, typemap};
use iron::modifiers::Header;
use time::{precise_time_ns, precise_time_s, now_utc};
use maud::{DOCTYPE, PreEscaped};
use rand::Rng;
use hyper::client::Client;
use hyper::header::ContentType;

struct Router {
    routes: HashMap<String, Box<Handler>>
}

struct ResponseTime;

impl typemap::Key for ResponseTime { type Value = u64; }

impl Router {
    fn new() -> Self {
        Router { routes: HashMap::new() }
    }

    fn add_route<S, H>(&mut self, path: S, handler: H)
        where S: Into<String>, H: Handler
    {
        self.routes.insert(path.into(), Box::new(handler));
    }
}

impl Handler for Router {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {
        req.extensions.insert::<ResponseTime>(precise_time_ns());

        let res = match self.routes.get(&req.url.path().join("/")) {
            Some(handler) => handler.handle(req),
            None => Ok(Response::with(status::NotFound))
        };

        let start_time = *req.extensions.get::<ResponseTime>().unwrap();
        let delta = precise_time_ns() - start_time;
        let now = now_utc();
        let now_str = now.rfc3339();
        let status = match res {
            Ok(ref r) if r.status.is_some() => r.status.unwrap().to_u16(),
            _ => 0
        };
        let path = "/".to_string() + &(req.url.path().join("/"));
        println!("{now} | {status:3} |\t{dur}ms | {method}\t{path}",
                 now = now_str,
                 status = status,
                 dur = (delta as f64) / 1000000.0,
                 method = req.method,
                 path = path
        );

        res
    }
}

struct IndexHandler {
    server_id: String,
    server_color: String,
}

impl Handler for IndexHandler {
    fn handle(&self, _: &mut Request) -> IronResult<Response> {
        Ok(Response::with((status::Ok,
                           get_markup(self.server_id.clone(), self.server_color.clone()))))
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Score {
    names: HashMap<String, u32>,
    last_submit_s: f64,
}

impl Score {
    fn new() -> Score {
        Score {
            names: HashMap::new(),
            last_submit_s: precise_time_s(),
        }
    }
}

struct ApiHandler {
    server_id: String,
    server_color: String,
    score: Arc<Mutex<Score>>,
    client: Arc<Client>,
    leaderboard_url: String,
}

impl ApiHandler {
    fn get_res_str(&self) -> String {
        format!("{{\"id\":\"{}\",\"color\":\"{}\"}}", self.server_id, self.server_color)
    }

    fn increase_score(&self, name: String) -> () {
        let mut score = self.score.lock().unwrap();
        println!("Increasing score {:?}", *score);
        let v = score.names.entry(name).or_insert(0);
        *v += 1;
    }

    fn try_submit_score(&self) -> () {
        let threshold = 5.0;
        let mut score = self.score.lock().unwrap();
        if precise_time_s() - score.last_submit_s < threshold {
            return;
        }

        let url = format!("{}/submit", self.leaderboard_url);
        println!("Submitting score {}...", url);

        let client = self.client.clone();
        let serialized = serde_json::to_string_pretty(&*score).unwrap();
        println!("{}", serialized);

        (*score).last_submit_s = precise_time_s();
        (*score).names.clear();

        thread::spawn(move || {
            let res = client.post(&url)
                .header(ContentType::json())
                .body(&serialized)
                .send();

            match res {
                Ok(_) => {
                    println!("Score was submitted");
                },
                Err(e) => {
                    println!("Failed to submit score {}", e);
                },
            }
        });
    }
}

impl Handler for ApiHandler {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {
        match req.method {
            method::Post => {},
            _ => return Ok(Response::with(status::MethodNotAllowed))
        }

        match req.get::<bodyparser::Json>() {
            Ok(Some(json_body)) => {
                match json_body.find("name") {
                    Some(n) if n.is_string() => {
                        // Somehow name is surrounded by quotes
                        let name = n.to_string().replace("\"", "");
                        println!("Name: {}", name);

                        self.increase_score(name);
                        self.try_submit_score();

                        Ok(Response::with((status::Ok,
                                           Header(ContentType::json()),
                                           self.get_res_str())))
                    }
                    _ => Ok(Response::with(status::BadRequest))
                }
            }
            _ => Ok(Response::with(status::BadRequest))
        }
    }
}


fn main() {
    let mut router = Router::new();
    let server_id = get_server_id();
    let server_color = get_server_color();

    let index = IndexHandler {
        server_id: server_id.clone(),
        server_color: server_color.clone(),
    };
    let api = ApiHandler {
        server_id: server_id.clone(),
        server_color: server_color.clone(),
        score: Arc::new(Mutex::new(Score::new())),
        client: Arc::new(Client::new()),
        leaderboard_url: env::var_os("LEADERBOARD_URL")
            .map_or(Ok("http://localhost:8080".into()), |s| s.into_string())
            .unwrap(),
    };
    println!("Server id='{}' and color='{}'", server_id, server_color);

    router.add_route("", index);
    router.add_route("submit", api);
    router.add_route("heartbeat", |_: &mut Request| -> IronResult<Response> {
        let now = now_utc();
        let now_str = format!("{}", now.rfc3339());
        Ok(Response::with((status::Ok, now_str)))
    });

    println!("Start listening on 3000...");
    Iron::new(router).http("0.0.0.0:3000").unwrap();
}

fn get_server_id() -> String {
    let letters = "abcdefghijklmnopqrstuvwxyz";
    let half = (letters.len() as f32 / 2.0) as usize;
    let i = rand::thread_rng().gen_range(0, half);
    let j = rand::thread_rng().gen_range(0, half);
    let mut chars = letters.chars();
    let a = chars.nth(i);
    let b = chars.nth(j);
    format!("{}{}", a.unwrap(), b.unwrap()).to_uppercase()
}

fn get_server_color() -> String {
    let colors = vec![
        "rgb(2,63,165)",
        "rgb(142,6,59)",
        "rgb(74,111,227)",
        "rgb(211,63,106)",
        "rgb(17,198,56)",
        "rgb(239,151,8)",
        "rgb(15,207,192)",
        "rgb(247,156,212)",
    ];
    let i = rand::thread_rng().gen_range(0, colors.len());
    colors[i].to_string()
}

fn get_markup(server_id: String, server_color: String) -> maud::PreEscaped<String> {
    let bg_style = format!("background-color: {};", server_color);

    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8" /
                title "Game page"
                style {
                  "body {"
                  "  padding: 0;"
                  "  margin: 0;"
                  "  background: #fff;"
                  "  font-size: 120%;"
                  "}"

                  "* {"
                  "  box-sizing: border-box;"
                  "}"

                  "main {"
                  "  max-width: 40rem;"
                  "  margin: 2rem auto;"
                  "}"

                  "main #name {"
                  "  font-size: 2rem;"
                  "  padding: 1rem;"
                  "  width: 100%;"
                  "}"

                  "main #submitBtn {"
                  "  padding: 2rem 0;"
                  "  width: 100%;"
                  "}"

                  ".stats {"
                  "  display: flex;"
                  "}"

                  ".stats #res {"
                  "  width: 5rem;"
                  "  height: 5rem;"
                  "  border-radius: 50%;"
                  "  padding-top: 1.2rem;"
                  "  text-align: center;"
                  "  color: #f7f7f7;"
                  "  font-size: 2rem;"
                  "}"
                }
            }

            body {
                main {
                    h1 "Game page"

                    p {
                        input#name type="text" placeholder="My first name is..." /
                    }

                    p {
                        button#submitBtn disabled? "Submit"
                    }

                    div.stats {
                        div#res style=(bg_style) (server_id)

                        ul {
                            li {
                                "Submitted: "
                                span#submitTimes "0"
                                " times"
                            }

                            li {
                                "Failed: "
                                span#failTimes "0"
                                " times"
                            }
                        }
                    }
                }

                script {
                  "var nameEl = document.querySelector('#name');"
                  "var submitBtnEl = document.querySelector('#submitBtn');"
                  "var submitTimesEl = document.querySelector('#submitTimes');"
                  "var failTimesEl = document.querySelector('#failTimes');"
                  "var resEl = document.querySelector('#res');"
                  "var submitTimes = 0;"
                  "var failTimes = 0;"

                  "submitBtnEl.disabled = nameEl.value.length === 0;"

                  "nameEl.addEventListener('keyup', function (e) {"
                  (PreEscaped("  if(e.key === 'Enter' && e.target.value.length) {"))
                  "    submit();"
                  "  }"
                  "  submitBtnEl.disabled = e.target.value.length === 0;"
                  "}, false);"

                  "submitBtnEl.addEventListener('click', submit, false);"

                  "function submit() {"
                  "  var name = nameEl.value;"
                  "  fetch('/submit', {"
                  "    method: 'POST',"
                  "    headers: {"
                  "      'Content-Type': 'application/json'"
                  "    },"
                  "    body: JSON.stringify({"
                  "      name: name"
                  "    })"
                  "  }).then(function (r) {"
                  "    if(r.status === 200) {"
                  "      submitTimes++;"
                  "      submitTimesEl.textContent = submitTimes;"
                  "    } else {"
                  "      failTimes++;"
                  "      failTimesEl.textContent = failTimes;"
                  "    }"
                  "    try {"
                  "      var j = r.json();"
                  "      j.then(function (json) {"
                  "        res.textContent = json.id;"
                  "        res.style.backgroundColor = json.color;"
                  "      });"
                  "    } catch (e) { console.error(e); }"
                  "  });"
                  "}"
                }
            }
        }
    }
}
