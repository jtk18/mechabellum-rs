//! Pseudonymous player IDs for a community server.
//!
//! A replay names each player by account ID (an 18-digit number whose low digits count up, so
//! the space is small enough to enumerate) and by display name (public: shown in game, on
//! leaderboards, in every opponent's replay). A plain hash of either can be reversed by hashing
//! candidates, so neither is used. The server holds a secret key instead, and a player's ID
//! becomes an HMAC of the account ID under that key: stable across renames and across
//! uploaders, so binding, caps and corroboration keep working, and not computable by anyone
//! without the key. Names are dropped entirely. The key lives in its own file, not in the
//! database, so a copy of the database alone doesn't carry it.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use std::path::Path;

/// Name stored for every player on a server that keeps no names.
pub const ANONYMOUS_NAME: &str = "Player";

/// Account IDs at least this long are also replaced wherever else they appear in a replay.
/// Real ones are 18 digits; shorter IDs (bots, local games) can collide with other numbers.
const LONG_ID: usize = 15;

/// Hex digits of the HMAC kept in a pseudonym: 80 bits, so collisions need around a trillion
/// players.
const ID_HEX: usize = 20;

pub struct Pseudonyms {
    key: [u8; 32],
}

impl Pseudonyms {
    pub fn from_key(key: [u8; 32]) -> Self {
        Pseudonyms { key }
    }

    /// Read the key from `path` (64 hex digits), or create one there when `create` allows and
    /// the file doesn't exist.
    pub fn load(path: &Path, create: bool) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let text = text.trim();
                let mut key = [0u8; 32];
                if text.len() != 64 {
                    bail!("{}: expected 64 hex digits", path.display());
                }
                for (i, byte) in key.iter_mut().enumerate() {
                    *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
                        .with_context(|| format!("{}: not hex", path.display()))?;
                }
                Ok(Pseudonyms { key })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                let mut key = [0u8; 32];
                getrandom::getrandom(&mut key).map_err(|e| anyhow::anyhow!("rng: {e}"))?;
                write_private(path, &format!("{}\n", hex(&key)))
                    .with_context(|| format!("writing {}", path.display()))?;
                log::warn!(
                    "created a new pseudonym key at {}; back it up with the database: a server \
                     without it can't match new uploads to the players already stored",
                    path.display()
                );
                Ok(Pseudonyms { key })
            }
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// The pseudonym for an account ID. Pseudonyms map to themselves, so rewriting twice is
    /// harmless.
    pub fn id(&self, account: &str) -> String {
        let account = account.trim();
        if is_pseudonym(account) {
            return account.to_owned();
        }
        let mac = hmac_sha256(&self.key, format!("player:{account}").as_bytes());
        format!("p{}", &hex(&mac)[..ID_HEX])
    }

    /// The pseudonym for a Steam ID, kept apart from account pseudonyms by its own prefix.
    pub fn steam(&self, steam_id: &str) -> String {
        let mac = hmac_sha256(&self.key, format!("steam:{}", steam_id.trim()).as_bytes());
        format!("s{}", &hex(&mac)[..ID_HEX])
    }

    /// A value that identifies the key without revealing it, stored in the database so a server
    /// started with a different key refuses to run instead of splitting every player in two.
    pub fn check_value(&self) -> String {
        hex(&hmac_sha256(&self.key, b"check"))[..16].to_owned()
    }

    /// The replay with every player's account ID replaced by its pseudonym and every name by
    /// [`ANONYMOUS_NAME`]. Account IDs sit in each `<PlayerRecord>`'s first `<id>` and in
    /// `<HostID>` (custom games); long ones are replaced anywhere else they appear too, so a
    /// field this code doesn't know about can't carry one through.
    pub fn document(&self, document: &str) -> String {
        let mut accounts: Vec<String> = Vec::new();
        let mut out = String::with_capacity(document.len());
        let mut rest = document;
        while let Some(start) = rest.find("<PlayerRecord>") {
            let end = rest[start..]
                .find("</PlayerRecord>")
                .map_or(rest.len(), |e| start + e);
            out.push_str(&rest[..start]);
            let mut record = rest[start..end].to_owned();
            if let Some(account) = replace_first(&mut record, "id", |raw| {
                let raw = raw.trim();
                (!raw.is_empty() && raw != "0").then(|| self.id(raw))
            }) {
                accounts.push(account);
            }
            replace_first(&mut record, "name", |_| Some(ANONYMOUS_NAME.to_owned()));
            out.push_str(&record);
            rest = &rest[end..];
        }
        out.push_str(rest);

        let mut rest = std::mem::take(&mut out);
        while let Some(start) = rest.find("<HostID>") {
            let (head, tail) = rest.split_at(start);
            out.push_str(head);
            let mut host = tail.to_owned();
            replace_first(&mut host, "HostID", |raw| {
                let raw = raw.trim();
                (!raw.is_empty() && raw != "0").then(|| self.id(raw))
            });
            let after = host
                .find("</HostID>")
                .map_or(host.len(), |e| e + "</HostID>".len());
            out.push_str(&host[..after]);
            rest = host[after..].to_owned();
        }
        out.push_str(&rest);

        for account in accounts {
            if account.len() >= LONG_ID && account.bytes().all(|b| b.is_ascii_digit()) {
                out = replace_number(&out, &account, &self.id(&account));
            }
        }
        out
    }
}

/// Whether `s` has the shape of a pseudonym: `p` and lowercase hex.
pub fn is_pseudonym(s: &str) -> bool {
    s.len() == 1 + ID_HEX
        && s.starts_with('p')
        && s[1..]
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Replace the text of the first `<tag>` element in `xml` with `with(old)`, when that returns
/// one. Returns the old text, trimmed, when it was replaced. An empty `<tag />` coming first
/// leaves everything alone, so a later element of the same name is never taken for it.
fn replace_first(
    xml: &mut String,
    tag: &str,
    with: impl FnOnce(&str) -> Option<String>,
) -> Option<String> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let first = [open.clone(), format!("<{tag}/>"), format!("<{tag} />")]
        .iter()
        .filter_map(|form| xml.find(form.as_str()).map(|at| (at, form == &open)))
        .min()?;
    if !first.1 {
        return None;
    }
    let a = first.0 + open.len();
    let b = a + xml[a..].find(&close)?;
    let old = xml[a..b].to_owned();
    let new = with(&old)?;
    xml.replace_range(a..b, &new);
    Some(old.trim().to_owned())
}

/// `text` with every occurrence of the number `from` that isn't part of a longer number
/// replaced by `to`.
fn replace_number(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(from) {
        let before = rest[..i].bytes().last();
        let after = rest[i + from.len()..].bytes().next();
        out.push_str(&rest[..i]);
        if before.is_some_and(|b| b.is_ascii_digit()) || after.is_some_and(|b| b.is_ascii_digit()) {
            out.push_str(from);
        } else {
            out.push_str(to);
        }
        rest = &rest[i + from.len()..];
    }
    out.push_str(rest);
    out
}

/// HMAC-SHA256 (RFC 2104) with a key of at most one block.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    assert!(key.len() <= BLOCK);
    let mut padded = [0u8; BLOCK];
    padded[..key.len()].copy_from_slice(key);
    let inner = Sha256::new()
        .chain_update(padded.map(|b| b ^ 0x36))
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(padded.map(|b| b ^ 0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Write `contents` to a new file only its owner can read.
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(contents.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mechabellum/fixtures");
        std::fs::read_to_string(dir.join(name)).unwrap()
    }

    /// RFC 4231 test cases 1 and 2.
    #[test]
    fn test_hmac_matches_rfc_4231() {
        assert_eq!(
            hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn test_pseudonyms_are_keyed_stable_and_idempotent() {
        let a = Pseudonyms::from_key([1; 32]);
        let b = Pseudonyms::from_key([2; 32]);
        let id = a.id("281474976711196086");
        assert!(is_pseudonym(&id), "{id}");
        assert_eq!(a.id(" 281474976711196086 "), id);
        assert_eq!(a.id(&id), id);
        assert_ne!(b.id("281474976711196086"), id);
        assert_ne!(a.id("281474976711196087"), id);
        assert_ne!(a.steam("281474976711196086")[1..], id[1..]);
        assert_ne!(a.check_value(), b.check_value());
    }

    /// No account ID or name survives the rewrite in any fixture, the result still parses to
    /// the same match with pseudonymous players, and a second rewrite changes nothing.
    #[test]
    fn test_documents_keep_no_account_ids_or_names() {
        let keys = Pseudonyms::from_key([7; 32]);
        for name in [
            "battle_record.xml",
            "battle_record2.xml",
            "battle_record_2324.xml",
            "battle_record_2340.xml",
            "4player_brawl.xml",
            "4player_brawl2.xml",
        ] {
            let raw = fixture(name);
            let before =
                mechabellum::BattleRecord::from_bytes(raw.as_bytes(), Path::new(name)).unwrap();
            let rewritten = keys.document(&raw);
            for player in &before.player_records.player_record {
                let id = player.id.trim();
                if id.len() >= LONG_ID {
                    assert!(!rewritten.contains(id), "{name}: {id} survived");
                }
                let shown = player.name.trim();
                if !shown.is_empty() && shown != ANONYMOUS_NAME {
                    assert!(
                        !rewritten.contains(&format!(">{shown}<")),
                        "{name}: a name survived"
                    );
                }
            }
            let after =
                mechabellum::BattleRecord::from_bytes(rewritten.as_bytes(), Path::new(name))
                    .unwrap();
            let pairs: Vec<_> = before
                .player_records
                .player_record
                .iter()
                .zip(&after.player_records.player_record)
                .collect();
            assert!(!pairs.is_empty(), "{name}");
            for (b, a) in pairs {
                if b.id.trim().len() >= LONG_ID {
                    assert_eq!(a.id.trim(), keys.id(b.id.trim()), "{name}");
                }
                // Bots have an empty `<name />`, which stays empty.
                let expected = if b.name.trim().is_empty() {
                    ""
                } else {
                    ANONYMOUS_NAME
                };
                assert_eq!(a.name.trim(), expected, "{name}");
            }
            assert_eq!(
                after.match_detail().rounds.len(),
                before.match_detail().rounds.len(),
                "{name}"
            );
            assert_eq!(keys.document(&rewritten), rewritten, "{name}");
        }
    }

    #[test]
    fn test_host_ids_and_stray_copies_are_replaced() {
        let keys = Pseudonyms::from_key([7; 32]);
        let account = "281474976711196086";
        let doc = format!(
            "<BattleRecord><HostID>{account}</HostID><PlayerRecords><PlayerRecord><id>{account}\
             </id><name>Someone</name><data><id>3</id></data></PlayerRecord><PlayerRecord><id>15\
             </id><name /><unit><name>Crawler</name></unit></PlayerRecord></PlayerRecords>\
             <Note>{account}</Note><Other>1{account}</Other></BattleRecord>"
        );
        let out = keys.document(&doc);
        let id = keys.id(account);
        assert_eq!(out.matches(&id).count(), 3, "{out}");
        assert!(
            out.contains(&format!("1{account}")),
            "a longer number is left alone"
        );
        assert!(out.contains("<data><id>3</id>"), "unit IDs are left alone");
        assert!(out.contains(&format!("<name>{ANONYMOUS_NAME}</name>")));
        assert!(
            out.contains("<name /><unit><name>Crawler</name>"),
            "a bot's empty name doesn't pass to a later element"
        );
    }

    #[test]
    fn test_key_file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pseudonym.key");
        assert!(Pseudonyms::load(&path, false).is_err());
        let made = Pseudonyms::load(&path, true).unwrap();
        let read = Pseudonyms::load(&path, false).unwrap();
        assert_eq!(made.check_value(), read.check_value());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(&path, "not a key").unwrap();
        assert!(Pseudonyms::load(&path, true).is_err());
    }
}
