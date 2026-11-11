//! `pscat fonts ...` (issue #599). Kept out of `main.rs`, which only
//! hands over `argv` after the word `fonts`.

use std::path::PathBuf;
use std::process::ExitCode;

use super::install::{self, Curl, Request, Source};
use super::{PINNED_INDEX, packs};

const USAGE: &str = "\
usage: pscat fonts install [--version <fonts-YYYY.MM[.N]>|latest] [--from <file>] [--pack <name>]...
       pscat fonts list
       pscat fonts remove [<pack>...]

  install   download, verify and install font packs. With no --version or
            --from, installs the pack release this pscat pins (reproducible
            renders). --version and --from install a user-selected release;
            --from takes a font-index.json (pack files beside it) or one
            .tar.zst pack. --pack limits it to the named packs (default: all).
            This is the only command in pscat that uses the network.
  list      show the pinned release, installed packs (re-verified against
            their manifests), and fonts/catalog overrides
  remove    remove installed packs (all of them when none are named)

Packs install into $PSCAT_HOME/fonts/packs, else the per-user data directory.";

/// Run `pscat fonts <args>`.
pub fn main(args: &[String]) -> ExitCode {
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("pscat fonts: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn install_dir() -> Result<PathBuf, String> {
    crate::paths::process_roots()
        .pack_install_dir()
        .ok_or_else(|| {
            "no place to install font packs: set PSCAT_HOME, or HOME (APPDATA on Windows)"
                .to_string()
        })
}

fn run(args: &[String]) -> Result<(), String> {
    let Some((cmd, rest)) = args.split_first() else {
        println!("{USAGE}");
        return Ok(());
    };
    match cmd.as_str() {
        "install" => cmd_install(rest),
        "list" => {
            if let Some(a) = rest.first() {
                return Err(format!("unexpected argument {a}"));
            }
            cmd_list()
        }
        "remove" => cmd_remove(rest),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command {other:?}\n{USAGE}")),
    }
}

fn cmd_install(args: &[String]) -> Result<(), String> {
    let mut version: Option<String> = None;
    let mut from: Option<PathBuf> = None;
    let mut pack_names = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("missing value after {flag}"))
        };
        match a.as_str() {
            "--version" => version = Some(value("--version")?),
            "--from" => from = Some(PathBuf::from(value("--from")?)),
            "--pack" => pack_names.push(value("--pack")?),
            other => return Err(format!("unexpected argument {other}\n{USAGE}")),
        }
    }
    let source = match (version, from) {
        (Some(_), Some(_)) => return Err("--version and --from are mutually exclusive".into()),
        (Some(v), None) if v == "latest" => Source::Latest,
        (Some(v), None) => Source::Tag(v),
        (None, Some(f)) => Source::File(f),
        (None, None) => Source::Pinned,
    };
    let dest = install_dir()?;
    let req = Request {
        source,
        packs: pack_names,
        dest: dest.clone(),
        pin: PINNED_INDEX,
        repo: env!("CARGO_PKG_REPOSITORY"),
    };
    let done = install::install(&req, &Curl)?;
    for note in &done.retired {
        eprintln!("{note}");
    }
    for p in &done.installed {
        println!(
            "installed {} {} ({}) in {}",
            p.name,
            p.tag,
            p.selection(PINNED_INDEX).as_str(),
            p.dir.display()
        );
    }
    Ok(())
}

fn cmd_list() -> Result<(), String> {
    match PINNED_INDEX {
        Some(p) => println!("pinned release: {} (index sha256 {})", p.tag, p.sha256),
        None => println!("pinned release: none (this build pins no fonts-* release)"),
    }
    match crate::paths::process_roots().pack_install_dir() {
        Some(d) => println!("install directory: {}", d.display()),
        None => println!("install directory: none (set PSCAT_HOME or HOME)"),
    }
    let overrides = crate::paths::catalog_dirs();
    for d in &overrides {
        println!(
            "override: {} (user-selected; its faces win over installed packs)",
            d.display()
        );
    }
    let installed = super::installed();
    let mut bad = false;
    for p in &installed {
        let check = match install::verify(p) {
            Ok(n) => format!("{n} files verified"),
            Err(problems) => {
                bad = true;
                format!("FAILED verification: {}", problems.join("; "))
            }
        };
        println!(
            "installed: {} {} {} at {}; {check}",
            p.name,
            p.tag,
            p.selection(PINNED_INDEX).as_str(),
            p.dir.display()
        );
    }
    for def in packs() {
        if !installed.iter().any(|p| p.name == def.name) {
            println!("not installed: {} ({})", def.name, def.description);
        }
    }
    if bad {
        return Err("an installed pack failed verification; reinstall it".into());
    }
    Ok(())
}

fn cmd_remove(args: &[String]) -> Result<(), String> {
    if let Some(flag) = args.iter().find(|a| a.starts_with('-')) {
        return Err(format!("unexpected argument {flag}"));
    }
    let dest = install_dir()?;
    let removed = install::remove(&dest, args)?;
    if removed.is_empty() {
        println!("no packs installed in {}", dest.display());
    }
    for n in removed {
        println!("removed {n} from {}", dest.display());
    }
    Ok(())
}
