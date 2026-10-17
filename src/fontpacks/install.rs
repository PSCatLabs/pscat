//! `pscat fonts install`/`remove`/`list` behind the CLI (issue #599).
//!
//! The chain of trust for the default install: the binary pins the
//! index's tag and SHA-256 ([`super::pinned`]); the index lists each
//! pack's SHA-256; each pack's `manifest.json` lists every file's
//! SHA-256. Everything is downloaded and checked into a staging
//! directory beside the install before anything is moved into place,
//! so a failure at any step leaves the previous install untouched and
//! no partial pack behind.
//!
//! Downloading goes through [`Fetch`], so tests drive the whole path
//! with an in-memory fetcher. The real one shells out to `curl` (see
//! [`Curl`] for why), the only network access anywhere in pscat.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::archive::{safe_path, untar, unzstd};
use super::{INDEX_SCHEMA, Installed, MANIFEST_SCHEMA, Pinned};

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Something that can GET a URL.
pub trait Fetch {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String>;
}

/// The real fetcher: `curl`, restricted to HTTPS (redirects included,
/// since GitHub release assets redirect to a CDN).
///
/// Why not an HTTP crate: a Rust TLS stack means rustls plus a root
/// store (webpki-roots is CDLA-Permissive-2.0, outside this project's
/// dependency-licence allowlist) or a platform verifier with its own
/// per-OS dependency tree, for one optional command. `curl` ships with
/// macOS, Windows 10+ and every mainstream Linux, uses the OS trust
/// store, and adds nothing to the binary. Integrity never rests on the
/// transport anyway: every byte is checked against the SHA-256 chain.
pub struct Curl;

impl Fetch for Curl {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        let out = std::process::Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--retry",
                "2",
                "--connect-timeout",
                "30",
                "--user-agent",
                concat!("pscat/", env!("CARGO_PKG_VERSION")),
                url,
            ])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    "`pscat fonts install` downloads with curl, which is not on PATH; \
                     install curl, or download the pack files yourself and use --from"
                        .to_string()
                } else {
                    format!("could not run curl: {e}")
                }
            })?;
        if !out.status.success() {
            return Err(format!(
                "download of {url} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(out.stdout)
    }
}

/// Where the packs come from.
#[derive(Debug, Clone)]
pub enum Source {
    /// The index this binary pins.
    Pinned,
    /// `--version <tag>`.
    Tag(String),
    /// `--version latest`: the newest `fonts-*` release.
    Latest,
    /// `--from <file>`: a `font-index.json` (packs beside it) or one
    /// `.tar.zst` pack.
    File(PathBuf),
}

pub struct Request<'a> {
    pub source: Source,
    /// Pack names; empty means every pack in the index.
    pub packs: Vec<String>,
    /// The `fonts/packs/` directory to install into.
    pub dest: PathBuf,
    pub pin: Option<Pinned>,
    /// `https://github.com/<owner>/<repo>`.
    pub repo: &'a str,
}

/// What got installed.
#[derive(Debug)]
pub struct Done {
    pub installed: Vec<Installed>,
}

pub fn valid_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix("fonts-") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    let digits = |s: &str, n: Option<usize>| {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && n.is_none_or(|n| s.len() == n)
    };
    match parts.as_slice() {
        [y, m] => digits(y, Some(4)) && digits(m, Some(2)),
        [y, m, n] => digits(y, Some(4)) && digits(m, Some(2)) && digits(n, None),
        _ => false,
    }
}

fn valid_pack_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn release_url(repo: &str, tag: &str, file: &str) -> String {
    format!("{repo}/releases/download/{tag}/{file}")
}

/// The newest `fonts-*` tag, from the GitHub releases API (newest
/// first). `/releases/latest` will not do: it can be a `v*` release.
fn latest_tag(repo: &str, fetch: &dyn Fetch) -> Result<String, String> {
    let path = repo
        .strip_prefix("https://github.com/")
        .ok_or_else(|| format!("cannot find releases for {repo}"))?;
    let url = format!("https://api.github.com/repos/{path}/releases?per_page=100");
    let v: Value = serde_json::from_slice(&fetch.fetch(&url)?)
        .map_err(|e| format!("release list from {url} is not JSON: {e}"))?;
    v.as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["draft"] != true && r["prerelease"] != true)
        .filter_map(|r| r["tag_name"].as_str())
        .find(|t| valid_tag(t))
        .map(str::to_string)
        .ok_or_else(|| "no fonts-* release has been published yet".to_string())
}

/// A parsed `font-index.json`.
#[derive(Debug)]
struct Index {
    tag: String,
    sha256: String,
    packs: Vec<IndexPack>,
}

#[derive(Debug)]
struct IndexPack {
    name: String,
    file: String,
    sha256: String,
}

fn parse_index(bytes: &[u8]) -> Result<Index, String> {
    let v: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("font index is not JSON: {e}"))?;
    let schema = v["index_schema"].as_u64().unwrap_or(0);
    if schema == 0 || schema > INDEX_SCHEMA {
        return Err(format!(
            "font index schema {schema} is not one this pscat reads (up to {INDEX_SCHEMA}); \
             a newer pscat may be needed"
        ));
    }
    let tag = v["tag"].as_str().unwrap_or_default().to_string();
    if !valid_tag(&tag) {
        return Err(format!("font index has an invalid tag {tag:?}"));
    }
    let mut packs = Vec::new();
    for p in v["packs"].as_array().into_iter().flatten() {
        let s = |k: &str| p[k].as_str().unwrap_or_default().to_string();
        let pack = IndexPack {
            name: s("name"),
            file: s("file"),
            sha256: s("sha256"),
        };
        if !valid_pack_name(&pack.name)
            || !safe_path(&pack.file)
            || pack.file.contains('/')
            || pack.sha256.len() != 64
        {
            return Err(format!(
                "font index has a malformed pack entry {}",
                pack.name
            ));
        }
        packs.push(pack);
    }
    if packs.is_empty() {
        return Err("font index lists no packs".into());
    }
    Ok(Index {
        tag,
        sha256: sha256_hex(bytes),
        packs,
    })
}

/// A pack checked and written to its staging directory.
struct Staged {
    name: String,
    tmp: PathBuf,
    installed: Installed,
}

/// Removes every staging directory it still holds when dropped, so an
/// early return anywhere leaves nothing behind.
struct Staging(Vec<Staged>);

impl Drop for Staging {
    fn drop(&mut self) {
        for s in &self.0 {
            let _ = std::fs::remove_dir_all(&s.tmp);
        }
    }
}

/// Check a pack's bytes against its manifest and write it to a fresh
/// staging directory under `dest`.
fn stage(
    dest: &Path,
    bytes: &[u8],
    expect_name: Option<&str>,
    expect_tag: Option<&str>,
    index_sha256: Option<&str>,
    source: &str,
    serial: usize,
) -> Result<Staged, String> {
    let entries = untar(&unzstd(bytes)?)?;
    let manifest_bytes = &entries
        .iter()
        .find(|e| e.path == "manifest.json")
        .ok_or("pack has no manifest.json")?
        .data;
    let m: Value = serde_json::from_slice(manifest_bytes)
        .map_err(|e| format!("pack manifest is not JSON: {e}"))?;
    let schema = m["manifest_schema"].as_u64().unwrap_or(0);
    if schema == 0 || schema > MANIFEST_SCHEMA {
        return Err(format!(
            "pack manifest schema {schema} is not one this pscat reads (up to {MANIFEST_SCHEMA}); \
             a newer pscat may be needed"
        ));
    }
    let name = m["pack"].as_str().unwrap_or_default().to_string();
    let tag = m["tag"].as_str().unwrap_or_default().to_string();
    if !valid_pack_name(&name) || !valid_tag(&tag) {
        return Err("pack manifest has no valid pack name and tag".into());
    }
    if let Some(want) = expect_name
        && want != name
    {
        return Err(format!("the index entry {want} holds a pack named {name}"));
    }
    if let Some(want) = expect_tag
        && want != tag
    {
        return Err(format!(
            "pack {name} is from {tag}, but the index is {want}"
        ));
    }
    let mut listed = std::collections::HashMap::new();
    for f in m["files"].as_array().into_iter().flatten() {
        let path = f["path"].as_str().unwrap_or_default();
        let sha = f["sha256"].as_str().unwrap_or_default();
        if !safe_path(path) || path == "manifest.json" || path == "install.json" {
            return Err(format!("pack manifest lists a bad path {path:?}"));
        }
        if listed.insert(path.to_string(), sha.to_string()).is_some() {
            return Err(format!("pack manifest lists {path} twice"));
        }
    }
    if listed.is_empty() {
        return Err(format!("pack {name} lists no files"));
    }
    for e in &entries {
        if e.path == "manifest.json" {
            continue;
        }
        let want = listed
            .get(&e.path)
            .ok_or_else(|| format!("{name}: {} is in the pack but not its manifest", e.path))?;
        let got = sha256_hex(&e.data);
        if &got != want {
            return Err(format!(
                "{name}: checksum mismatch for {} (manifest {want}, file {got}); refusing the pack",
                e.path
            ));
        }
    }
    for path in listed.keys() {
        if !entries.iter().any(|e| &e.path == path) {
            return Err(format!(
                "{name}: {path} is in the manifest but not the pack"
            ));
        }
    }

    std::fs::create_dir_all(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    let tmp = dest.join(format!(".staging-{name}-{}-{serial}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let staged = Staged {
        name: name.clone(),
        tmp: tmp.clone(),
        installed: Installed {
            name: name.clone(),
            dir: dest.join(&name),
            tag: tag.clone(),
            pack_sha256: sha256_hex(bytes),
            index_sha256: index_sha256.map(str::to_string),
            source: source.to_string(),
        },
    };
    let write = || -> std::io::Result<()> {
        for e in &entries {
            let p = tmp.join(&e.path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&p, &e.data)?;
        }
        let record = json!({
            "install_schema": 1,
            "pack": name,
            "tag": tag,
            "pack_sha256": staged.installed.pack_sha256,
            "index_sha256": staged.installed.index_sha256,
            "source": source,
        });
        std::fs::write(
            tmp.join("install.json"),
            serde_json::to_string_pretty(&record).unwrap_or_default() + "\n",
        )
    };
    if let Err(e) = write() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("cannot write {}: {e}", tmp.display()));
    }
    Ok(staged)
}

/// Move staged packs into place. Each pack swaps in with renames
/// (old aside, new in, old removed); a failed rename puts the old one
/// back. Verification is all or nothing; this phase is per pack, so a
/// rename failing on one pack keeps the packs already swapped in. The
/// packs not yet reached stay in `staging`, whose drop removes them.
fn commit(mut staging: Staging) -> Result<Vec<Installed>, String> {
    let mut done = Vec::new();
    staging.0.reverse();
    while let Some(s) = staging.0.pop() {
        let final_dir = s.installed.dir.clone();
        let aside = final_dir.with_file_name(format!(".old-{}-{}", s.name, std::process::id()));
        let had_old = final_dir.exists();
        if had_old {
            let _ = std::fs::remove_dir_all(&aside);
            if let Err(e) = std::fs::rename(&final_dir, &aside) {
                let _ = std::fs::remove_dir_all(&s.tmp);
                return Err(format!("cannot replace {}: {e}", final_dir.display()));
            }
        }
        if let Err(e) = std::fs::rename(&s.tmp, &final_dir) {
            if had_old {
                let _ = std::fs::rename(&aside, &final_dir);
            }
            let _ = std::fs::remove_dir_all(&s.tmp);
            return Err(format!("cannot install {}: {e}", final_dir.display()));
        }
        if had_old {
            let _ = std::fs::remove_dir_all(&aside);
        }
        done.push(s.installed);
    }
    Ok(done)
}

/// Install per `req`. Nothing is moved into place until every
/// requested pack has been fetched and verified.
pub fn install(req: &Request, fetch: &dyn Fetch) -> Result<Done, String> {
    for p in &req.packs {
        if !valid_pack_name(p) {
            return Err(format!("{p:?} is not a pack name"));
        }
    }
    // A lone pack file: no index, so only its own manifest vouches for it.
    if let Source::File(path) = &req.source
        && path.extension().is_none_or(|e| e != "json")
    {
        let bytes =
            std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let s = stage(
            &req.dest,
            &bytes,
            None,
            None,
            None,
            &path.display().to_string(),
            0,
        )?;
        if !req.packs.is_empty() && !req.packs.contains(&s.name) {
            let _ = std::fs::remove_dir_all(&s.tmp);
            return Err(format!(
                "{} holds pack {}, not {:?}",
                path.display(),
                s.name,
                req.packs
            ));
        }
        return Ok(Done {
            installed: commit(Staging(vec![s]))?,
        });
    }

    // Everything else goes through an index.
    type Origin<'f> = Box<dyn Fn(&str) -> Result<(Vec<u8>, String), String> + 'f>;
    let (index_bytes, origin): (Vec<u8>, Origin) = match &req.source {
        Source::File(path) => {
            let bytes =
                std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            (
                bytes,
                Box::new(move |file: &str| {
                    let p = dir.join(file);
                    std::fs::read(&p)
                        .map(|b| (b, p.display().to_string()))
                        .map_err(|e| format!("cannot read {}: {e}", p.display()))
                }),
            )
        }
        Source::Pinned | Source::Tag(_) | Source::Latest => {
            let tag = match &req.source {
                Source::Pinned => req
                    .pin
                    .ok_or(
                        "this pscat build pins no font-pack release yet; use \
                             `pscat fonts install --version <fonts-YYYY.MM>` or `--version latest` \
                             (reported as user-selected), or `--from <file>`",
                    )?
                    .tag
                    .to_string(),
                Source::Tag(t) => t.clone(),
                _ => latest_tag(req.repo, fetch)?,
            };
            if !valid_tag(&tag) {
                return Err(format!("{tag:?} is not a fonts-YYYY.MM[.N] tag"));
            }
            let url = release_url(req.repo, &tag, "font-index.json");
            let bytes = fetch.fetch(&url)?;
            let repo = req.repo.to_string();
            (
                bytes,
                Box::new(move |file: &str| {
                    let url = release_url(&repo, &tag, file);
                    fetch.fetch(&url).map(|b| (b, url))
                }),
            )
        }
    };
    let index = parse_index(&index_bytes)?;
    if let Source::Pinned = req.source
        && let Some(pin) = req.pin
        && (index.sha256 != pin.sha256 || index.tag != pin.tag)
    {
        return Err(format!(
            "the {} font index does not match the checksum this pscat pins \
             (pinned {}, got {}); refusing to install",
            pin.tag, pin.sha256, index.sha256
        ));
    }
    if let Source::Tag(t) = &req.source
        && t != &index.tag
    {
        return Err(format!("asked for {t}, but the index says {}", index.tag));
    }
    let wanted: Vec<&IndexPack> = if req.packs.is_empty() {
        index.packs.iter().collect()
    } else {
        req.packs
            .iter()
            .map(|n| {
                index.packs.iter().find(|p| &p.name == n).ok_or_else(|| {
                    let names: Vec<&str> = index.packs.iter().map(|p| p.name.as_str()).collect();
                    format!(
                        "{} has no pack {n} (it has {})",
                        index.tag,
                        names.join(", ")
                    )
                })
            })
            .collect::<Result<_, _>>()?
    };
    let mut staging = Staging(Vec::new());
    for (i, p) in wanted.iter().enumerate() {
        let (bytes, from) = origin(&p.file)?;
        let got = sha256_hex(&bytes);
        if got != p.sha256 {
            return Err(format!(
                "checksum mismatch for {} (index {}, download {got}); refusing to install",
                p.file, p.sha256
            ));
        }
        staging.0.push(stage(
            &req.dest,
            &bytes,
            Some(&p.name),
            Some(&index.tag),
            Some(&index.sha256),
            &from,
            i,
        )?);
    }
    Ok(Done {
        installed: commit(staging)?,
    })
}

/// Remove installed packs from `dest` (all of them when `names` is
/// empty). Only directories carrying an `install.json` are touched.
pub fn remove(dest: &Path, names: &[String]) -> Result<Vec<String>, String> {
    let installed = super::installed_in(dest);
    let targets: Vec<&Installed> = if names.is_empty() {
        installed.iter().collect()
    } else {
        names
            .iter()
            .map(|n| {
                installed
                    .iter()
                    .find(|p| &p.name == n)
                    .ok_or_else(|| format!("pack {n} is not installed in {}", dest.display()))
            })
            .collect::<Result<_, _>>()?
    };
    let mut removed = Vec::new();
    for p in targets {
        std::fs::remove_dir_all(&p.dir)
            .map_err(|e| format!("cannot remove {}: {e}", p.dir.display()))?;
        removed.push(p.name.clone());
    }
    Ok(removed)
}

/// Re-check an installed pack's files against its manifest: the
/// number of files verified, or the problems found.
pub fn verify(p: &Installed) -> Result<usize, Vec<String>> {
    let m: Value = match std::fs::read(p.dir.join("manifest.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
    {
        Some(m) => m,
        None => return Err(vec!["manifest.json is missing or unreadable".into()]),
    };
    let mut problems = Vec::new();
    let mut n = 0;
    for f in m["files"].as_array().into_iter().flatten() {
        let path = f["path"].as_str().unwrap_or_default();
        let want = f["sha256"].as_str().unwrap_or_default();
        if !safe_path(path) {
            problems.push(format!("bad path {path:?}"));
            continue;
        }
        match std::fs::read(p.dir.join(path)) {
            Ok(b) if sha256_hex(&b) == want => n += 1,
            Ok(_) => problems.push(format!("{path}: checksum mismatch")),
            Err(_) => problems.push(format!("{path}: missing")),
        }
    }
    if problems.is_empty() {
        Ok(n)
    } else {
        Err(problems)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::archive::tests::{tar_of, zstd_of};
    use super::*;
    use std::collections::HashMap;

    pub(crate) const REPO: &str = "https://github.com/o/r";

    /// A pack file holding `files` (path, bytes) with an honest manifest.
    pub(crate) fn pack(name: &str, tag: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
        let manifest = json!({
            "manifest_schema": 1,
            "pack": name,
            "tag": tag,
            "files": files.iter().map(|(p, b)| json!({
                "path": p, "kind": "font", "license": "OFL-1.1",
                "copyright": "test", "sha256": sha256_hex(b), "size": b.len(),
            })).collect::<Vec<_>>(),
        })
        .to_string();
        let mut entries: Vec<(&str, &[u8], u8)> =
            files.iter().map(|(p, b)| (*p, *b, b'0')).collect();
        entries.push(("manifest.json", manifest.as_bytes(), b'0'));
        zstd_of(&tar_of(&entries))
    }

    pub(crate) fn index(tag: &str, packs: &[(&str, &[u8])]) -> Vec<u8> {
        json!({
            "index_schema": 1,
            "tag": tag,
            "packs": packs.iter().map(|(n, b)| json!({
                "name": n, "file": format!("{n}-{tag}.tar.zst"),
                "sha256": sha256_hex(b), "size": b.len(), "manifest_schema": 1,
            })).collect::<Vec<_>>(),
        })
        .to_string()
        .into_bytes()
    }

    pub(crate) struct Fake(pub HashMap<String, Vec<u8>>);
    impl Fetch for Fake {
        fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
            self.0.get(url).cloned().ok_or_else(|| format!("404 {url}"))
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "pscat-fontpacks-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn release(tag: &str) -> (Fake, Vec<u8>) {
        let a = pack("alpha", tag, &[("A/a-Regular.ttf", b"alpha font")]);
        let b = pack(
            "beta",
            tag,
            &[("B/b-Regular.ttf", b"beta font"), ("B/OFL.txt", b"ofl")],
        );
        let idx = index(tag, &[("alpha", &a), ("beta", &b)]);
        let mut m = HashMap::new();
        m.insert(
            format!("{REPO}/releases/download/{tag}/font-index.json"),
            idx.clone(),
        );
        m.insert(
            format!("{REPO}/releases/download/{tag}/alpha-{tag}.tar.zst"),
            a,
        );
        m.insert(
            format!("{REPO}/releases/download/{tag}/beta-{tag}.tar.zst"),
            b,
        );
        (Fake(m), idx)
    }

    fn req(source: Source, dest: &Path, pin: Option<Pinned>) -> Request<'static> {
        Request {
            source,
            packs: Vec::new(),
            dest: dest.to_path_buf(),
            pin,
            repo: REPO,
        }
    }

    fn leftovers(dest: &Path) -> Vec<String> {
        std::fs::read_dir(dest)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn pinned_install_from_a_release_url_verifies_and_reports_pinned() {
        let dest = scratch("pinned");
        let (fake, idx) = release("fonts-2026.10");
        let sha: &'static str = Box::leak(sha256_hex(&idx).into_boxed_str());
        let pin = Pinned {
            tag: "fonts-2026.10",
            sha256: sha,
        };
        let done = install(&req(Source::Pinned, &dest, Some(pin)), &fake).unwrap();
        assert_eq!(done.installed.len(), 2);
        let installed = super::super::installed_in(&dest);
        assert_eq!(installed.len(), 2);
        for p in &installed {
            assert_eq!(p.selection(Some(pin)), super::super::Selection::Pinned);
            assert!(verify(p).is_ok());
        }
        assert_eq!(
            std::fs::read(dest.join("beta/B/b-Regular.ttf")).unwrap(),
            b"beta font"
        );
        let mut names = leftovers(&dest);
        names.sort();
        assert_eq!(names, ["alpha", "beta"], "no staging dirs left behind");

        // --version of another tag: user-selected even with packs of the same name.
        let (fake2, _) = release("fonts-2026.11");
        install(
            &req(Source::Tag("fonts-2026.11".into()), &dest, Some(pin)),
            &fake2,
        )
        .unwrap();
        for p in super::super::installed_in(&dest) {
            assert_eq!(p.tag, "fonts-2026.11");
            assert_eq!(
                p.selection(Some(pin)),
                super::super::Selection::UserSelected
            );
        }
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn pinned_install_refuses_an_index_that_does_not_match_the_pin() {
        let dest = scratch("badpin");
        let (fake, _) = release("fonts-2026.10");
        let pin = Pinned {
            tag: "fonts-2026.10",
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };
        let err = install(&req(Source::Pinned, &dest, Some(pin)), &fake).unwrap_err();
        assert!(err.contains("does not match the checksum"), "{err}");
        assert!(leftovers(&dest).is_empty());
        let err = install(&req(Source::Pinned, &dest, None), &fake).unwrap_err();
        assert!(err.contains("pins no font-pack release"), "{err}");
    }

    #[test]
    fn a_checksum_mismatch_is_refused_and_nothing_partial_is_left() {
        let dest = scratch("mismatch");
        let tag = "fonts-2026.10";
        // A good install first, so we can see it survives a bad one.
        let (fake, _) = release(tag);
        install(&req(Source::Tag(tag.into()), &dest, None), &fake).unwrap();
        let before = std::fs::read(dest.join("alpha/A/a-Regular.ttf")).unwrap();

        // beta's bytes differ from what the index promises; alpha is fine
        // and is fetched first, so it would be staged before the failure.
        let (mut fake, _) = release(tag);
        let key = format!("{REPO}/releases/download/{tag}/beta-{tag}.tar.zst");
        fake.0.get_mut(&key).unwrap().push(0);
        let err = install(&req(Source::Tag(tag.into()), &dest, None), &fake).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        let mut names = leftovers(&dest);
        names.sort();
        assert_eq!(names, ["alpha", "beta"], "no staging dir survives");
        assert_eq!(
            std::fs::read(dest.join("alpha/A/a-Regular.ttf")).unwrap(),
            before
        );

        // A file whose bytes disagree with its own manifest.
        let mut bad = pack("gamma", tag, &[("G/g.ttf", b"good")]);
        let tarred = untar(&unzstd(&bad).unwrap()).unwrap();
        let manifest = &tarred
            .iter()
            .find(|e| e.path == "manifest.json")
            .unwrap()
            .data;
        bad = zstd_of(&tar_of(&[
            ("G/g.ttf", b"evil", b'0'),
            ("manifest.json", manifest, b'0'),
        ]));
        let file = dest.join("gamma.tar.zst");
        std::fs::write(&file, &bad).unwrap();
        let err = install(&req(Source::File(file.clone()), &dest, None), &fake).unwrap_err();
        assert!(err.contains("checksum mismatch for G/g.ttf"), "{err}");
        assert!(!dest.join("gamma").exists());
        assert!(!leftovers(&dest).iter().any(|n| n.starts_with('.')));
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn local_index_and_lone_pack_files_install() {
        let dest = scratch("local");
        let src = scratch("local-src");
        std::fs::create_dir_all(&src).unwrap();
        let tag = "fonts-2026.10";
        let a = pack("alpha", tag, &[("A/a.ttf", b"a")]);
        let idx = index(tag, &[("alpha", &a)]);
        std::fs::write(src.join(format!("alpha-{tag}.tar.zst")), &a).unwrap();
        std::fs::write(src.join("font-index.json"), &idx).unwrap();
        let sha: &'static str = Box::leak(sha256_hex(&idx).into_boxed_str());
        let pin = Pinned { tag, sha256: sha };
        let none = Fake(HashMap::new());
        install(
            &req(Source::File(src.join("font-index.json")), &dest, Some(pin)),
            &none,
        )
        .unwrap();
        let p = &super::super::installed_in(&dest)[0];
        assert_eq!(p.selection(Some(pin)), super::super::Selection::Pinned);

        // The same pack on its own has no index to vouch for it.
        install(
            &req(
                Source::File(src.join(format!("alpha-{tag}.tar.zst"))),
                &dest,
                Some(pin),
            ),
            &none,
        )
        .unwrap();
        let p = &super::super::installed_in(&dest)[0];
        assert_eq!(p.index_sha256, None);
        assert_eq!(
            p.selection(Some(pin)),
            super::super::Selection::UserSelected
        );

        assert_eq!(remove(&dest, &[]).unwrap(), ["alpha"]);
        assert!(super::super::installed_in(&dest).is_empty());
        assert!(remove(&dest, &["alpha".into()]).is_err());
        let _ = std::fs::remove_dir_all(&dest);
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn latest_picks_the_newest_fonts_release_and_selects_packs() {
        let dest = scratch("latest");
        let (mut fake, _) = release("fonts-2026.11");
        fake.0.insert(
            "https://api.github.com/repos/o/r/releases?per_page=100".into(),
            json!([
                {"tag_name": "v0.6.0"},
                {"tag_name": "fonts-2026.12", "prerelease": true},
                {"tag_name": "fonts-2026.11"},
                {"tag_name": "fonts-2026.10"},
            ])
            .to_string()
            .into_bytes(),
        );
        let mut r = req(Source::Latest, &dest, None);
        r.packs = vec!["beta".into()];
        let done = install(&r, &fake).unwrap();
        assert_eq!(done.installed.len(), 1);
        assert_eq!(done.installed[0].tag, "fonts-2026.11");
        assert!(dest.join("beta").is_dir() && !dest.join("alpha").exists());
        r.packs = vec!["nope".into()];
        assert!(install(&r, &fake).unwrap_err().contains("has no pack nope"));
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn tags_and_schemas_are_checked() {
        assert!(valid_tag("fonts-2026.10"));
        assert!(valid_tag("fonts-2026.10.1"));
        for bad in [
            "v0.6.0",
            "fonts-26.10",
            "fonts-2026.1",
            "fonts-2026.10.",
            "fonts-latest",
        ] {
            assert!(!valid_tag(bad), "{bad}");
        }
        let future = json!({"index_schema": 99, "tag": "fonts-2026.10", "packs": []}).to_string();
        assert!(
            parse_index(future.as_bytes())
                .unwrap_err()
                .contains("newer pscat")
        );
    }
}
