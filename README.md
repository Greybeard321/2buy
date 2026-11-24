# 2buy

A tiny shared shopping list with one SQLite database and personal bookmark URLs.
See `SPEC.md` for the full specification.

## Build

On the target machine (e.g. a Raspberry Pi):

```sh
cargo build --release
sudo install -m 0755 target/release/2buy /usr/local/bin/2buy
```

No runtime dependencies other than libc and the SQLite file; SQLite is bundled into
the binary.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `BIND_ADDR` | `127.0.0.1:8080` | Plain HTTP listen address; TLS is handled by your reverse proxy |
| `DATABASE_PATH` | `2buy.sqlite3` | SQLite database file |
| `BASE_URL` | `http://localhost:8080` | Public `https://…` URL of the reverse proxy, used when printing links |
| `LIST_TITLE` | `Shopping list` | Heading shown in the web app |

## Admin CLI

```sh
export DATABASE_PATH=/var/lib/2buy/2buy.sqlite3
export BASE_URL=https://shopping.example.com

2buy generate-url alice     # create a new URL (ID + URL printed once)
2buy list-tokens            # ID, username, token prefix, creation time, status
2buy show-url <token-id>    # reprint the exact stored URL, repeatable
2buy revoke <token-id>      # permanently revoke one URL
2buy list-items             # inspect current items
2buy serve                  # run the HTTP server
```

Multiple tokens per username are supported. Revoking one token does not affect the
others. A revoked token cannot be reactivated; generate a new URL instead.

## Running as a service

An example unit is in `deploy/2buy.service`. Adjust user, paths and environment, then:

```sh
sudo install -d -o 2buy -g 2buy /var/lib/2buy
sudo install -m 0644 deploy/2buy.service /etc/systemd/system/2buy.service
sudo systemctl daemon-reload
sudo systemctl enable --now 2buy
```

Point your existing reverse proxy at `127.0.0.1:8080` and set `BASE_URL` to the
public HTTPS address it serves.

### Serving under a URL prefix

If 2buy is mounted at a subpath such as `/2buy`, include it in `BASE_URL` and let
the reverse proxy strip it. 2buy then emits redirects with the prefix included,
and the frontend keeps the prefix in its API calls:

```sh
export BASE_URL=https://shopping.example.com/2buy
```

```nginx
location /2buy/ {
    proxy_pass http://127.0.0.1:8080/;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

No `proxy_redirect` or API rewrite rules are needed.

## Tests

```sh
./scripts/test.sh
```

Runs Rust unit/integration tests (including a real server process) and the Node
tests for the pure frontend logic.

## License

Released into the public domain under the Unlicense; see `UNLICENSE`.
