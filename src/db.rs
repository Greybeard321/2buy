use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::{error::Error, fmt, fs, path::Path};

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ','now')";

#[derive(Clone, Debug)]
pub struct TokenRow {
    pub id: String,
    pub username: String,
    pub token: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: i64,
    pub name: String,
    pub quantity: String,
    pub category: String,
    pub comment: String,
    pub disabled: bool,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemView {
    pub id: i64,
    pub name: String,
    pub quantity: String,
    pub category: String,
    pub comment: String,
    pub disabled: bool,
}

impl From<Item> for ItemView {
    fn from(item: Item) -> Self {
        Self {
            id: item.id,
            name: item.name,
            quantity: item.quantity,
            category: item.category,
            comment: item.comment,
            disabled: item.disabled,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ItemFields {
    pub name: String,
    pub quantity: String,
    pub category: String,
    pub comment: String,
    pub disabled: bool,
}

#[derive(Debug)]
pub enum UpdateError {
    NotFound,
    Conflict,
    Db(rusqlite::Error),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "item not found"),
            Self::Conflict => write!(f, "an item with that name already exists"),
            Self::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl Error for UpdateError {}

impl From<rusqlite::Error> for UpdateError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Db(value)
    }
}

pub fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn token_prefix(token: &str) -> String {
    token.chars().take(8).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn open(path: &Path) -> Result<Connection, Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(&format!(
        "
        PRAGMA journal_mode=WAL;
        PRAGMA busy_timeout=5000;

        CREATE TABLE IF NOT EXISTS tokens (
            id          TEXT PRIMARY KEY,
            username    TEXT NOT NULL,
            token       TEXT NOT NULL UNIQUE,
            created_at  TEXT NOT NULL DEFAULT ({NOW}),
            revoked_at  TEXT
        );

        CREATE TABLE IF NOT EXISTS items (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            name         TEXT NOT NULL,
            quantity     TEXT NOT NULL,
            category     TEXT NOT NULL,
            comment      TEXT NOT NULL DEFAULT '',
            disabled     INTEGER NOT NULL DEFAULT 0 CHECK (disabled IN (0, 1)),
            created_by   TEXT NOT NULL,
            created_at   TEXT NOT NULL DEFAULT ({NOW}),
            updated_at   TEXT NOT NULL DEFAULT ({NOW}),
            name_key     TEXT NOT NULL UNIQUE,
            category_key TEXT NOT NULL
        );
        "
    ))?;
    Ok(conn)
}

fn row_to_token(row: &rusqlite::Row<'_>) -> rusqlite::Result<TokenRow> {
    Ok(TokenRow {
        id: row.get(0)?,
        username: row.get(1)?,
        token: row.get(2)?,
        created_at: row.get(3)?,
        revoked_at: row.get(4)?,
    })
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get(0)?,
        name: row.get(1)?,
        quantity: row.get(2)?,
        category: row.get(3)?,
        comment: row.get(4)?,
        disabled: row.get::<_, i64>(5)? != 0,
        created_by: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

const ITEM_COLUMNS: &str =
    "id, name, quantity, category, comment, disabled, created_by, created_at, updated_at";
const TOKEN_COLUMNS: &str = "id, username, token, created_at, revoked_at";

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn random_token_id(conn: &Connection) -> Result<String, rusqlite::Error> {
    loop {
        let mut bytes = [0u8; 4];
        OsRng.fill_bytes(&mut bytes);
        let id = hex(&bytes);
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tokens WHERE id = ?1)",
            params![id],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(id);
        }
    }
}

pub fn create_token(conn: &mut Connection, username: &str) -> Result<TokenRow, rusqlite::Error> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let id = random_token_id(&tx)?;
    let token = loop {
        let candidate = random_token();
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tokens WHERE token = ?1)",
            params![candidate],
            |row| row.get(0),
        )?;
        if !exists {
            break candidate;
        }
    };
    tx.execute(
        "INSERT INTO tokens (id, username, token) VALUES (?1, ?2, ?3)",
        params![id, username, token],
    )?;
    let row = tx.query_row(
        &format!("SELECT {TOKEN_COLUMNS} FROM tokens WHERE id = ?1"),
        params![id],
        row_to_token,
    )?;
    tx.commit()?;
    Ok(row)
}

pub fn list_tokens(
    conn: &Connection,
    username_filter: Option<&str>,
) -> Result<Vec<TokenRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TOKEN_COLUMNS} FROM tokens WHERE (?1 IS NULL OR username = ?1) ORDER BY created_at, id"
    ))?;
    let rows = stmt.query_map(params![username_filter], row_to_token)?;
    rows.collect()
}

pub fn find_token(conn: &Connection, id: &str) -> Result<Option<TokenRow>, rusqlite::Error> {
    conn.query_row(
        &format!("SELECT {TOKEN_COLUMNS} FROM tokens WHERE id = ?1"),
        params![id],
        row_to_token,
    )
    .optional()
}

pub fn authenticate(conn: &Connection, username: &str, token: &str) -> Result<bool, rusqlite::Error> {
    let row: Option<TokenRow> = conn
        .query_row(
            &format!("SELECT {TOKEN_COLUMNS} FROM tokens WHERE token = ?1"),
            params![token],
            row_to_token,
        )
        .optional()?;
    Ok(match row {
        Some(row) => row.username == username && row.revoked_at.is_none(),
        None => false,
    })
}

pub fn revoke_token(conn: &Connection, id: &str) -> Result<bool, rusqlite::Error> {
    let changed = conn.execute(
        &format!("UPDATE tokens SET revoked_at = ({NOW}) WHERE id = ?1 AND revoked_at IS NULL"),
        params![id],
    )?;
    Ok(changed > 0)
}

pub fn token_display(row: &TokenRow) -> String {
    format!(
        "{:<10} {:<12} {:<10} {:<25} {}",
        row.id,
        row.username,
        token_prefix(&row.token),
        row.created_at,
        if row.revoked_at.is_some() { "revoked" } else { "active" }
    )
}

pub fn get_item(conn: &Connection, id: i64) -> Result<Option<Item>, rusqlite::Error> {
    conn.query_row(
        &format!("SELECT {ITEM_COLUMNS} FROM items WHERE id = ?1"),
        params![id],
        row_to_item,
    )
    .optional()
}

pub fn list_items(conn: &Connection) -> Result<Vec<Item>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("SELECT {ITEM_COLUMNS} FROM items ORDER BY id"))?;
    let rows = stmt.query_map([], row_to_item)?;
    rows.collect()
}

pub fn create_or_update_item(
    conn: &mut Connection,
    fields: &ItemFields,
    username: &str,
) -> Result<(ItemView, bool), rusqlite::Error> {
    let name = fields.name.trim();
    let category = fields.category.trim();
    let name_key = normalize(name);
    let category_key = normalize(category);

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing_id: Option<i64> = tx
        .query_row(
            "SELECT id FROM items WHERE name_key = ?1",
            params![name_key],
            |row| row.get(0),
        )
        .optional()?;

    let (id, created) = if let Some(id) = existing_id {
        tx.execute(
            "UPDATE items
                SET name = ?1, quantity = ?2, category = ?3, category_key = ?4,
                    comment = ?5, disabled = ?6, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
              WHERE id = ?7",
            params![
                name,
                fields.quantity,
                category,
                category_key,
                fields.comment,
                fields.disabled as i64,
                id
            ],
        )?;
        (id, false)
    } else {
        tx.execute(
            "INSERT INTO items
                (name, quantity, category, comment, disabled, created_by, name_key, category_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                name,
                fields.quantity,
                category,
                fields.comment,
                fields.disabled as i64,
                username,
                name_key,
                category_key
            ],
        )?;
        (tx.last_insert_rowid(), true)
    };
    tx.commit()?;

    let item = get_item(conn, id)?.expect("item just written must exist");
    Ok((item.into(), created))
}

pub fn update_item(
    conn: &mut Connection,
    id: i64,
    fields: &ItemFields,
) -> Result<ItemView, UpdateError> {
    let name = fields.name.trim();
    let category = fields.category.trim();
    let name_key = normalize(name);
    let category_key = normalize(category);

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: Option<i64> = tx
        .query_row("SELECT id FROM items WHERE id = ?1", params![id], |row| row.get(0))
        .optional()?;
    if exists.is_none() {
        return Err(UpdateError::NotFound);
    }
    let conflict: Option<i64> = tx
        .query_row(
            "SELECT id FROM items WHERE name_key = ?1 AND id <> ?2",
            params![name_key, id],
            |row| row.get(0),
        )
        .optional()?;
    if conflict.is_some() {
        return Err(UpdateError::Conflict);
    }

    tx.execute(
        "UPDATE items
            SET name = ?1, quantity = ?2, category = ?3, category_key = ?4,
                comment = ?5, disabled = ?6, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
          WHERE id = ?7",
        params![
            name,
            fields.quantity,
            category,
            category_key,
            fields.comment,
            fields.disabled as i64,
            id
        ],
    )?;
    tx.commit()?;
    let item = get_item(conn, id)?.expect("item just updated must exist");
    Ok(item.into())
}

pub fn set_disabled(
    conn: &Connection,
    id: i64,
    disabled: bool,
) -> Result<ItemView, UpdateError> {
    let changed = conn.execute(
        "UPDATE items
            SET disabled = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
          WHERE id = ?2",
        params![disabled as i64, id],
    )?;
    if changed == 0 {
        return Err(UpdateError::NotFound);
    }
    let item = get_item(conn, id)?.expect("item just updated must exist");
    Ok(item.into())
}

pub fn delete_item(conn: &Connection, id: i64) -> Result<bool, rusqlite::Error> {
    let changed = conn.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    Ok(changed > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        open(Path::new(":memory:")).unwrap()
    }

    fn fields(name: &str, category: &str) -> ItemFields {
        ItemFields {
            name: name.to_string(),
            quantity: "1".to_string(),
            category: category.to_string(),
            comment: String::new(),
            disabled: false,
        }
    }

    #[test]
    fn normalize_trims_and_lowercases() {
        assert_eq!(normalize("  Hardware Store "), "hardware store");
        assert_eq!(normalize("MiLk"), "milk");
    }

    #[test]
    fn token_rows_are_bound_and_revocable() {
        let mut c = conn();
        let alice = create_token(&mut c, "alice").unwrap();
        let bob = create_token(&mut c, "bob").unwrap();
        assert!(authenticate(&mut c, "alice", &alice.token).unwrap());
        assert!(!authenticate(&mut c, "bob", &alice.token).unwrap());
        assert!(revoke_token(&mut c, &alice.id).unwrap());
        assert!(!authenticate(&mut c, "alice", &alice.token).unwrap());
        assert!(authenticate(&mut c, "bob", &bob.token).unwrap());
    }

    #[test]
    fn upsert_is_case_insensitive_and_re_enables() {
        let mut c = conn();
        let mut f = fields("Milk", "Dairy");
        let (first, created) = create_or_update_item(&mut c, &f, "alice").unwrap();
        assert!(created);
        assert!(set_disabled(&mut c, first.id, true).unwrap().disabled);

        f.name = "milk".to_string();
        f.quantity = "2".to_string();
        f.category = "dairy".to_string();
        let (second, created) = create_or_update_item(&mut c, &f, "bob").unwrap();
        assert!(!created);
        assert_eq!(second.id, first.id);
        assert_eq!(second.name, "milk");
        assert!(!second.disabled);
        assert_eq!(list_items(&c).unwrap().len(), 1);
    }

    #[test]
    fn rename_conflict_and_not_found() {
        let mut c = conn();
        let a = create_or_update_item(&mut c, &fields("Milk", "Dairy"), "alice")
            .unwrap()
            .0;
        let b = create_or_update_item(&mut c, &fields("Butter", "Dairy"), "alice")
            .unwrap()
            .0;
        let mut rename = fields("MILK", "Dairy");
        assert!(matches!(
            update_item(&mut c, b.id, &rename),
            Err(UpdateError::Conflict)
        ));
        rename.name = "Cream".to_string();
        assert_eq!(update_item(&mut c, b.id, &rename).unwrap().id, b.id);
        assert!(matches!(
            update_item(&mut c, 999, &rename),
            Err(UpdateError::NotFound)
        ));
        assert_eq!(list_items(&c).unwrap().len(), 2);
        assert_eq!(a.name, "Milk");
    }
}
