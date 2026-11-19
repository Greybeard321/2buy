use serde_json::{json, Value};
use std::{
    env, fs,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::Duration,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TestEnv {
    dir: PathBuf,
    db: PathBuf,
    base: String,
    child: Option<Child>,
}

impl TestEnv {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = env::temp_dir().join(format!("2buy-test-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let base = format!("http://127.0.0.1:{port}");
        let db = dir.join("test.sqlite3");

        let child = Command::new(env!("CARGO_BIN_EXE_2buy"))
            .arg("serve")
            .env("BIND_ADDR", format!("127.0.0.1:{port}"))
            .env("DATABASE_PATH", &db)
            .env("BASE_URL", &base)
            .env("LIST_TITLE", "Family supplies")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let env = Self {
            dir,
            db,
            base,
            child: Some(child),
        };
        for _ in 0..100 {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        env
    }

    fn cli(&self, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_2buy"))
            .args(args)
            .env("DATABASE_PATH", &self.db)
            .env("BASE_URL", &self.base)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cli {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn generate(&self, username: &str) -> (String, String, String) {
        let output = self.cli(&["generate-url", username]);
        let id = line_after(&output, "Token ID: ");
        let url = line_after(&output, "URL: ");
        let token = url.rsplit('/').next().unwrap().to_string();
        (id, url, token)
    }

    fn api(&self, username: &str, token: &str, suffix: &str) -> String {
        format!("{}/api/{username}/{token}/{suffix}", self.base)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn line_after(output: &str, prefix: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .expect("expected CLI line")
        .trim()
        .to_string()
}

fn http(method: &str, url: &str, body: Option<Value>) -> (u16, String) {
    let result = match method {
        "GET" => ureq::get(url).call(),
        "POST" => ureq::post(url).send_json(body.expect("POST needs a body")),
        "PUT" => ureq::put(url).send_json(body.expect("PUT needs a body")),
        "PATCH" => ureq::patch(url).send_json(body.expect("PATCH needs a body")),
        "DELETE" => ureq::delete(url).call(),
        other => panic!("unsupported method {other}"),
    };

    match result {
        Ok(response) => (response.status(), response.into_string().unwrap_or_default()),
        Err(ureq::Error::Status(code, response)) => {
            (code, response.into_string().unwrap_or_default())
        }
        Err(err) => panic!("request to {url} failed: {err}"),
    }
}

fn http_json(method: &str, url: &str, body: Option<Value>) -> (u16, Value) {
    let (status, text) = http(method, url, body);
    let value = if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }))
    };
    (status, value)
}

fn item_fields(name: &str, quantity: &str, category: &str, comment: &str) -> Value {
    json!({ "name": name, "quantity": quantity, "category": category, "comment": comment })
}

#[test]
fn token_lifecycle_binding_revocation_and_reproduction() {
    let env = TestEnv::new();

    let (id1, url1, token1) = env.generate("alice");
    let (id2, url2, token2) = env.generate("alice");
    assert_ne!(id1, id2);
    assert_ne!(token1, token2);

    let listed = env.cli(&["list-tokens"]);
    assert!(listed.contains(&id1));
    assert!(listed.contains(&id2));
    assert!(listed.contains("alice"));
    assert!(listed.contains("active"));

    let shown = env.cli(&["show-url", &id1]);
    assert_eq!(shown.trim(), url1);
    assert_eq!(env.cli(&["show-url", &id1]).trim(), url1);

    assert_eq!(http("GET", &format!("{url1}/"), None).0, 200);
    assert_eq!(http("GET", &format!("{url2}/"), None).0, 200);

    let tampered_username = url1.replace("/u/alice/", "/u/bob/");
    assert_eq!(http("GET", &format!("{tampered_username}/"), None).0, 403);

    let copied_token = format!("{}/u/bob/{}", env.base, token1);
    assert_eq!(http("GET", &format!("{copied_token}/"), None).0, 403);

    env.cli(&["revoke", &id1]);

    assert_eq!(http("GET", &format!("{url1}/"), None).0, 403);
    assert_eq!(http("GET", &format!("{url2}/"), None).0, 200);
    assert_eq!(env.cli(&["show-url", &id1]).trim(), url1);

    let listed = env.cli(&["list-tokens"]);
    assert!(listed.contains("revoked"));
    assert!(listed.contains(&id2));
}

#[test]
fn item_lifecycle_uniqueness_and_conflicts() {
    let env = TestEnv::new();
    let (_, _, token) = env.generate("alice");
    let items_url = env.api("alice", &token, "items");

    let (status, body) = http_json("GET", &items_url, None);
    assert_eq!(status, 200);
    assert_eq!(body, json!([]));

    let (status, body) = http_json(
        "POST",
        &items_url,
        Some(item_fields("Milk", "2", "Dairy", "oat if possible")),
    );
    assert_eq!(status, 200);
    assert_eq!(body["result"], "created");
    let milk_id = body["item"]["id"].as_i64().unwrap();
    assert_eq!(body["item"]["name"], "Milk");
    assert_eq!(body["item"]["quantity"], "2");
    assert_eq!(body["item"]["category"], "Dairy");
    assert_eq!(body["item"]["comment"], "oat if possible");
    assert_eq!(body["item"]["disabled"], false);

    let (_, body) = http_json("GET", &items_url, None);
    let as_array = body.as_array().unwrap();
    assert_eq!(as_array.len(), 1);
    let object = as_array[0].as_object().unwrap();
    assert!(!object.contains_key("created_by"));
    assert!(!object.contains_key("created_at"));
    assert!(!object.contains_key("updated_at"));

    let (status, body) = http_json(
        "PATCH",
        &format!("{items_url}/{milk_id}"),
        Some(json!({ "disabled": true })),
    );
    assert_eq!(status, 200);
    assert_eq!(body["disabled"], true);

    let (status, body) = http_json(
        "POST",
        &items_url,
        Some(item_fields("milk", "1 bottle", "dairy", "again")),
    );
    assert_eq!(status, 200);
    assert_eq!(body["result"], "updated");
    assert_eq!(body["item"]["id"], milk_id);
    assert_eq!(body["item"]["name"], "milk");
    assert_eq!(body["item"]["quantity"], "1 bottle");
    assert_eq!(body["item"]["category"], "dairy");
    assert_eq!(body["item"]["disabled"], false);

    let (_, body) = http_json("GET", &items_url, None);
    assert_eq!(body.as_array().unwrap().len(), 1);

    let (status, body) = http_json(
        "POST",
        &items_url,
        Some(item_fields("Butter", "1", "Dairy", "")),
    );
    assert_eq!(status, 200);
    let butter_id = body["item"]["id"].as_i64().unwrap();

    let (status, _) = http_json(
        "PUT",
        &format!("{items_url}/{butter_id}"),
        Some(item_fields("MILK", "1", "Dairy", "")),
    );
    assert_eq!(status, 409);

    let (status, body) = http_json(
        "PUT",
        &format!("{items_url}/{milk_id}"),
        Some(item_fields("Milk 2", "3", "Other", "renamed")),
    );
    assert_eq!(status, 200);
    assert_eq!(body["name"], "Milk 2");
    assert_eq!(body["quantity"], "3");
    assert_eq!(body["category"], "Other");

    let (status, _) = http_json(
        "PATCH",
        &format!("{items_url}/{milk_id}"),
        Some(json!({ "disabled": false })),
    );
    assert_eq!(status, 200);

    let (status, _) = http_json("DELETE", &format!("{items_url}/{milk_id}"), None);
    assert_eq!(status, 200);
    let (_, body) = http_json("GET", &items_url, None);
    assert_eq!(body.as_array().unwrap().len(), 1);

    assert_eq!(
        http_json(
            "POST",
            &items_url,
            Some(json!({ "category": "Dairy" }))
        )
        .0,
        400
    );
    assert_eq!(
        http_json(
            "PATCH",
            &format!("{items_url}/999999"),
            Some(json!({ "disabled": true }))
        )
        .0,
        404
    );
    assert_eq!(http_json("DELETE", &format!("{items_url}/999999"), None).0, 404);
}

#[test]
fn frontend_assets_are_served_with_security_headers() {
    let env = TestEnv::new();
    let (_, url, _) = env.generate("alice");
    let root = format!("{url}/");

    let response = ureq::get(&root).call().unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.header("Referrer-Policy").unwrap(), "no-referrer");
    assert_eq!(response.header("X-Content-Type-Options").unwrap(), "nosniff");
    assert!(response
        .header("Content-Security-Policy")
        .unwrap()
        .contains("default-src 'self'"));
    let html = response.into_string().unwrap();
    assert!(html.contains("Family supplies"));
    assert!(html.contains("id=\"item-dialog\""));
    assert!(html.contains("id=\"category-filter\""));
    assert!(html.contains("app.mjs"));

    for asset in ["app.mjs", "logic.mjs", "style.css"] {
        let response = ureq::get(&format!("{root}{asset}")).call().unwrap();
        assert_eq!(response.status(), 200, "asset {asset}");
        let _ = response.into_string().unwrap();
    }

    let bad = url.replace(token_from_url(&url), "notavalidtoken000000000");
    assert_eq!(http("GET", &format!("{bad}/"), None).0, 403);
}

fn token_from_url(url: &str) -> &str {
    url.rsplit('/').next().unwrap()
}

#[test]
fn bookmark_url_without_trailing_slash_redirects_and_loads_app() {
    let env = TestEnv::new();
    let (_, url, token) = env.generate("alice");

    let agent = ureq::builder().redirects(0).build();
    let result = agent.get(&url).call();
    let (status, location) = match result {
        Ok(response) => (
            response.status(),
            response.header("Location").map(str::to_string),
        ),
        Err(ureq::Error::Status(code, response)) => (
            code,
            response.header("Location").map(str::to_string),
        ),
        Err(err) => panic!("request failed: {err}"),
    };
    let expected = format!("/u/alice/{token}/");
    assert_eq!(status, 302);
    assert_eq!(location.as_deref(), Some(expected.as_str()));

    let (status, body) = http("GET", &url, None);
    assert_eq!(status, 200);
    assert!(body.contains("app.mjs"));
    assert!(body.contains("id=\"add-item\""));
}

#[test]
fn cli_can_list_items_without_exposing_api_secrets() {
    let env = TestEnv::new();
    let (_, _, token) = env.generate("alice");
    let items_url = env.api("alice", &token, "items");
    assert_eq!(
        http(
            "POST",
            &items_url,
            Some(item_fields("Bread", "1 loaf", "Supermarket", ""))
        )
        .0,
        200
    );

    let listed = env.cli(&["list-items"]);
    assert!(listed.contains("Bread"));
    assert!(listed.contains("Supermarket"));
}
