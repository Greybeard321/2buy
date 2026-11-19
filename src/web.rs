use crate::{auth, config::Config, db};
use serde::Deserialize;
use serde_json::json;
use std::{error::Error, io::Cursor, str::FromStr};
use tiny_http::{Header, Request, Response, Server};

const INDEX: &str = include_str!("../static/index.html");
const APP: &str = include_str!("../static/app.mjs");
const LOGIC: &str = include_str!("../static/logic.mjs");
const STYLE: &str = include_str!("../static/style.css");

type Bytes = Cursor<Vec<u8>>;

#[derive(Deserialize, Default)]
struct RawItem {
    name: Option<String>,
    quantity: Option<String>,
    category: Option<String>,
    comment: Option<String>,
    disabled: Option<bool>,
}

fn header(name: &'static str, value: &'static str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn make_response(
    status: u32,
    body: String,
    content_type: &'static str,
) -> Response<Bytes> {
    let mut response = Response::from_string(body).with_status_code(status);
    response.add_header(header("Content-Type", content_type));
    response.add_header(header("Referrer-Policy", "no-referrer"));
    response.add_header(header("X-Content-Type-Options", "nosniff"));
    response.add_header(header("Cache-Control", "no-store"));
    response.add_header(header(
        "Content-Security-Policy",
        "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; form-action 'self'; base-uri 'none'",
    ));
    response
}

fn text(status: u32, body: &str) -> Response<Bytes> {
    make_response(status, body.to_string(), "text/plain; charset=utf-8")
}

fn json(status: u32, body: serde_json::Value) -> Response<Bytes> {
    make_response(status, body.to_string(), "application/json")
}

fn error(status: u32, message: &str) -> Response<Bytes> {
    json(status, json!({ "error": message }))
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn validate(raw: RawItem) -> Result<db::ItemFields, String> {
    let name = raw.name.unwrap_or_default().trim().to_string();
    if name.is_empty() {
        return Err("name is required".to_string());
    }
    if name.chars().count() > 120 {
        return Err("name is too long (max 120 characters)".to_string());
    }

    let category = raw.category.unwrap_or_default().trim().to_string();
    if category.is_empty() {
        return Err("category is required".to_string());
    }
    if category.chars().count() > 80 {
        return Err("category is too long (max 80 characters)".to_string());
    }

    let quantity = raw.quantity.unwrap_or_default().trim().to_string();
    let quantity = if quantity.is_empty() {
        "1".to_string()
    } else {
        quantity
    };
    if quantity.chars().count() > 40 {
        return Err("quantity is too long (max 40 characters)".to_string());
    }

    let comment = raw.comment.unwrap_or_default().trim().to_string();
    if comment.chars().count() > 500 {
        return Err("comment is too long (max 500 characters)".to_string());
    }

    Ok(db::ItemFields {
        name,
        quantity,
        category,
        comment,
        disabled: raw.disabled.unwrap_or(false),
    })
}

fn read_body(request: &mut Request) -> Result<String, Box<dyn Error>> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    Ok(body)
}

fn parse_item_body(request: &mut Request) -> Result<db::ItemFields, Response<Bytes>> {
    let body = read_body(request).map_err(|_| error(400, "could not read request body"))?;
    let raw: RawItem =
        serde_json::from_str(&body).map_err(|_| error(400, "request body must be valid JSON"))?;
    validate(raw).map_err(|message| error(400, &message))
}

fn serve_asset(tail: &[String], config: &Config) -> Response<Bytes> {
    if tail.is_empty() {
        let html = INDEX.replace("__LIST_TITLE__", &html_escape(&config.list_title));
        return make_response(200, html, "text/html; charset=utf-8");
    }
    match tail[0].as_str() {
        "app.mjs" => make_response(200, APP.to_string(), "text/javascript; charset=utf-8"),
        "logic.mjs" => make_response(200, LOGIC.to_string(), "text/javascript; charset=utf-8"),
        "style.css" => make_response(200, STYLE.to_string(), "text/css; charset=utf-8"),
        _ => text(404, "Not found"),
    }
}

fn handle_api(
    request: &mut Request,
    route: &auth::AuthPath,
    config: &Config,
) -> Result<Response<Bytes>, Box<dyn Error>> {
    if route.tail.first().map(String::as_str) != Some("items") {
        return Ok(text(404, "Not found"));
    }

    let mut conn = db::open(&config.database_path)?;
    let method = request.method().as_str().to_string();

    if route.tail.len() == 1 {
        return match method.as_str() {
            "GET" => {
                let items: Vec<db::ItemView> = db::list_items(&conn)?.into_iter().map(Into::into).collect();
                Ok(json(200, serde_json::to_value(items)?))
            }
            "POST" => {
                let fields = match parse_item_body(request) {
                    Ok(fields) => fields,
                    Err(response) => return Ok(response),
                };
                let (item, created) = db::create_or_update_item(&mut conn, &fields, &route.username)?;
                Ok(json(
                    200,
                    json!({
                        "item": item,
                        "result": if created { "created" } else { "updated" },
                    }),
                ))
            }
            _ => Ok(text(405, "Method not allowed")),
        };
    }

    if route.tail.len() != 2 {
        return Ok(text(404, "Not found"));
    }
    let id = match i64::from_str(&route.tail[1]) {
        Ok(id) => id,
        Err(_) => return Ok(error(400, "invalid item id")),
    };

    match method.as_str() {
        "PUT" => {
            let fields = match parse_item_body(request) {
                Ok(fields) => fields,
                Err(response) => return Ok(response),
            };
            match db::update_item(&mut conn, id, &fields) {
                Ok(item) => Ok(json(200, serde_json::to_value(item)?)),
                Err(db::UpdateError::NotFound) => Ok(error(404, "item not found")),
                Err(db::UpdateError::Conflict) => Ok(error(409, "an item with that name already exists")),
                Err(e) => Err(Box::new(e)),
            }
        }
        "PATCH" => {
            let body = read_body(request)?;
            #[derive(Deserialize)]
            struct PatchBody {
                disabled: Option<bool>,
            }
            let patch: PatchBody = match serde_json::from_str(&body) {
                Ok(patch) => patch,
                Err(_) => return Ok(error(400, "request body must be valid JSON")),
            };
            let disabled = match patch.disabled {
                Some(value) => value,
                None => return Ok(error(400, "disabled must be true or false")),
            };
            match db::set_disabled(&conn, id, disabled) {
                Ok(item) => Ok(json(200, serde_json::to_value(item)?)),
                Err(db::UpdateError::NotFound) => Ok(error(404, "item not found")),
                Err(e) => Err(Box::new(e)),
            }
        }
        "DELETE" => match db::delete_item(&conn, id)? {
            true => Ok(json(200, json!({ "ok": true }))),
            false => Ok(error(404, "item not found")),
        },
        _ => Ok(text(405, "Method not allowed")),
    }
}

fn handle(request: &mut Request, config: &Config) -> Result<Response<Bytes>, Box<dyn Error>> {
    let url = request.url().split('?').next().unwrap_or("/");
    let Some((is_api, route)) = auth::parse_path(url) else {
        return Ok(text(404, "Not found"));
    };

    let conn = db::open(&config.database_path)?;
    if !db::authenticate(&conn, &route.username, &route.token)? {
        return Ok(text(403, "Forbidden"));
    }
    drop(conn);

    if is_api {
        handle_api(request, &route, config)
    } else {
        let method = request.method().as_str();
        if method != "GET" {
            return Ok(text(405, "Method not allowed"));
        }

        // Bookmark URLs have no trailing slash. Relative asset paths in index.html
        // must resolve under /u/<username>/<token>/, so canonicalize with a redirect.
        if route.tail.is_empty() && !url.ends_with('/') {
            let location = format!("/u/{}/{}/", route.username, route.token);
            let mut response = text(302, "Redirecting to the canonical URL");
            response.add_header(
                Header::from_bytes(&b"Location"[..], location.as_bytes())
                    .expect("valid Location header"),
            );
            return Ok(response);
        }

        Ok(serve_asset(&route.tail, config))
    }
}

pub fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let server = Server::http(config.bind_addr.as_str())
        .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
    println!("2buy listening on http://{}", config.bind_addr);
    for mut request in server.incoming_requests() {
        let response = match handle(&mut request, &config) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("request failed: {err}");
                error(500, "internal server error")
            }
        };
        if let Err(err) = request.respond(response) {
            eprintln!("could not send response: {err}");
        }
    }
    Ok(())
}
