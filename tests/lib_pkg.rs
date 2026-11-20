//! `pscat lib install` and `pscat lib list` (issue #1056): a library of your
//! own, copied where `(lib/NAME/x.ps) run` finds it from any directory.
//! Every run gets `PSCAT_HOME` (or `HOME`) pointed at a scratch directory, so
//! nothing here touches the real per-user data directory. The MCP side is in
//! `tests/mcp.rs`.

mod common;
use common::CommandDeadline as _;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// `pscat ARGS` in `cwd`, installing into `$PSCAT_HOME = home`.
fn pscat(home: &Path, cwd: &Path, args: &[&str]) -> Output {
    base_command(cwd)
        .env("PSCAT_HOME", home)
        .env("HOME", home.join("unused-home"))
        .args(args)
        .output_deadline()
        .unwrap()
}

/// As [`pscat`] with no `PSCAT_HOME`: the per-user data directory is under
/// `user`.
fn pscat_user(user: &Path, cwd: &Path, args: &[&str]) -> Output {
    base_command(cwd)
        .env("HOME", user)
        .env("APPDATA", user)
        .args(args)
        .output_deadline()
        .unwrap()
}

fn base_command(cwd: &Path) -> Command {
    let mut c = Command::new(BIN);
    c.current_dir(cwd)
        .env_remove("PSCAT_HOME")
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .env_remove("XDG_DATA_HOME")
        .env("PSCAT_NO_CHECKOUT", "1")
        .env("PSCAT_TRACE_RESOURCES", "1");
    c
}

/// Where the per-user data directory is when `HOME` (`APPDATA`) is `user`.
fn user_data(user: &Path) -> PathBuf {
    if cfg!(windows) {
        user.join("pscat")
    } else if cfg!(target_os = "macos") {
        user.join("Library/Application Support/pscat")
    } else {
        user.join(".local/share/pscat")
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn ok(o: Output) -> String {
    assert!(
        o.status.success(),
        "stdout: {}\nstderr: {}",
        out(&o),
        err(&o)
    );
    out(&o)
}

/// A package that loads a sibling file from its own directory, by
/// `currentfilename`, and has a file in a subdirectory.
fn make_package(parent: &Path, name: &str) -> PathBuf {
    let dir = parent.join(name);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(
        dir.join("main.ps"),
        r"/dirname { dup length { dup 0 eq { pop pop () exit } if
    1 sub 2 copy get 47 eq { 1 add 0 exch getinterval exit } if } loop } def
/concat { /b exch def /a exch def
    a length b length add string dup 0 a putinterval dup a length b putinterval } def
(main: ) print currentfilename =
currentfilename dirname (sib.ps) concat run
currentfilename dirname (sub/deep.ps) concat run
",
    )
    .unwrap();
    std::fs::write(dir.join("sib.ps"), "(sib: ) print currentfilename =\n").unwrap();
    std::fs::write(
        dir.join("sub/deep.ps"),
        "(deep: ) print currentfilename =\n",
    )
    .unwrap();
    std::fs::write(dir.join(".hidden"), "not copied").unwrap();
    dir
}

fn main_lines(name: &str) -> Vec<String> {
    vec![
        format!("main: lib/{name}/main.ps"),
        format!("sib: lib/{name}/sib.ps"),
        format!("deep: lib/{name}/sub/deep.ps"),
    ]
}

fn lines(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

#[test]
fn an_installed_package_loads_from_any_directory() {
    let base = common::scratch_dir("lib-install");
    let home = base.join("home");
    let pkg = make_package(&base.join("src"), "mypkg");
    // The install itself runs from yet another directory.
    let elsewhere = base.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let text = ok(pscat(
        &home,
        &elsewhere,
        &["lib", "install", pkg.to_str().unwrap()],
    ));
    let dest = std::fs::canonicalize(home.join("lib/mypkg")).unwrap();
    assert!(text.contains("installed lib/mypkg: 3 files"), "{text}");
    assert!(text.contains(dest.to_str().unwrap()), "{text}");
    assert!(text.contains("(lib/mypkg/main.ps) run"), "{text}");
    assert!(dest.join("sub/deep.ps").is_file());
    assert!(!dest.join(".hidden").exists(), "dotfiles are not copied");

    // Loaded by name from a directory that has no lib/ and no copy of it,
    // and the siblings are found from the name the library was run by.
    let cwd = base.join("run-here");
    std::fs::create_dir_all(&cwd).unwrap();
    let ran = pscat(
        &home,
        &cwd,
        &["--headless", "-e", "(lib/mypkg/main.ps) run"],
    );
    let stderr = err(&ran);
    assert_eq!(lines(&ok(ran)), main_lines("mypkg"), "{stderr}");
    assert!(
        stderr.contains("pscat: resource lib/mypkg/main.ps <- pscat-home ("),
        "{stderr}"
    );

    // The same under read roots, as the MCP server runs every program: the
    // install directory is a read root, and the package is found in it.
    let confined = pscat(
        &home,
        &cwd,
        &[
            "--headless",
            "--read-root",
            home.to_str().unwrap(),
            "-e",
            "(lib/mypkg/main.ps) run",
        ],
    );
    assert_eq!(lines(&ok(confined)), main_lines("mypkg"));
}

#[test]
fn without_pscat_home_it_goes_to_the_per_user_data_directory() {
    let base = common::scratch_dir("lib-install-user");
    let user = base.join("user");
    let pkg = make_package(&base.join("src"), "tool-kit");
    ok(pscat_user(
        &user,
        &base,
        &["lib", "install", pkg.to_str().unwrap()],
    ));
    let dest = user_data(&user).join("lib/tool-kit");
    assert!(dest.join("main.ps").is_file(), "{}", dest.display());
    let cwd = base.join("run-here");
    std::fs::create_dir_all(&cwd).unwrap();
    let ran = pscat_user(
        &user,
        &cwd,
        &["--headless", "-e", "(lib/tool-kit/main.ps) run"],
    );
    assert_eq!(lines(&ok(ran)), main_lines("tool-kit"));
}

#[test]
fn name_overrides_the_directory_name_and_list_shows_packages() {
    let base = common::scratch_dir("lib-list");
    let home = base.join("home");
    let a = make_package(&base.join("src"), "alpha");
    let b = make_package(&base.join("src"), "beta");
    let empty = ok(pscat(&home, &base, &["lib", "list"]));
    assert!(empty.contains("no packages installed"), "{empty}");
    ok(pscat(
        &home,
        &base,
        &["lib", "install", a.to_str().unwrap(), "--name", "zeta"],
    ));
    ok(pscat(
        &home,
        &base,
        &["lib", "install", b.to_str().unwrap()],
    ));
    let list = ok(pscat(&home, &base, &["lib", "list"]));
    let l: Vec<&str> = list.lines().collect();
    assert!(l[0].starts_with("install directory: "), "{list}");
    assert!(l[1].starts_with("lib/beta: 3 files at "), "{list}");
    assert!(l[1].contains(home.join("lib/beta").file_name().unwrap().to_str().unwrap()));
    assert!(l[1].contains("from "), "{list}");
    assert!(l[2].starts_with("lib/zeta: 3 files at "), "{list}");
    assert_eq!(l.len(), 3, "{list}");
    assert!(!home.join("lib/alpha").exists());
    // A hand-made directory is listed, and said not to be ours.
    std::fs::create_dir_all(home.join("lib/mine")).unwrap();
    std::fs::write(home.join("lib/mine/x.ps"), "").unwrap();
    let list = ok(pscat(&home, &base, &["lib", "list"]));
    assert!(
        list.contains("lib/mine: 1 file at") && list.contains("not made by pscat lib install"),
        "{list}"
    );
}

#[test]
fn installing_again_replaces_the_package_without_keeping_stale_files() {
    let base = common::scratch_dir("lib-replace");
    let home = base.join("home");
    let pkg = make_package(&base.join("src"), "mypkg");
    ok(pscat(
        &home,
        &base,
        &["lib", "install", pkg.to_str().unwrap()],
    ));
    let dest = std::fs::canonicalize(home.join("lib/mypkg")).unwrap();
    // The package changes: one file gone, one changed, one new.
    std::fs::remove_file(pkg.join("sib.ps")).unwrap();
    std::fs::write(pkg.join("sub/deep.ps"), "(deep v2)=\n").unwrap();
    std::fs::write(pkg.join("new.ps"), "1 pop\n").unwrap();
    let text = ok(pscat(
        &home,
        &base,
        &["lib", "install", pkg.to_str().unwrap()],
    ));
    assert!(text.contains("replaced lib/mypkg: 3 files"), "{text}");
    assert!(
        text.contains("previous install's 3 files are gone"),
        "{text}"
    );
    assert!(text.contains("removed: sib.ps"), "{text}");
    assert!(!dest.join("sib.ps").exists(), "a stale file was merged in");
    assert_eq!(
        std::fs::read_to_string(dest.join("sub/deep.ps")).unwrap(),
        "(deep v2)=\n"
    );
    assert!(dest.join("new.ps").is_file());
    // No staging or backup left beside it.
    let beside: Vec<_> = std::fs::read_dir(home.join("lib"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(beside, ["mypkg"]);
    // A directory pscat lib did not make is never replaced.
    std::fs::create_dir_all(home.join("lib/mine")).unwrap();
    std::fs::write(home.join("lib/mine/keep.ps"), "keep").unwrap();
    let o = pscat(
        &home,
        &base,
        &["lib", "install", pkg.to_str().unwrap(), "--name", "mine"],
    );
    assert!(!o.status.success());
    assert!(
        err(&o).contains("not made by pscat lib install"),
        "{}",
        err(&o)
    );
    assert_eq!(
        std::fs::read_to_string(home.join("lib/mine/keep.ps")).unwrap(),
        "keep"
    );
}

/// The required security test: an install cannot create a file that shadows
/// a library built into pscat, by name or by file, in any spelling of the
/// name. (A file in the install directory would outrank the compiled-in
/// copy in the `lib/` search, for the CLI and the MCP server alike.)
#[test]
fn an_install_cannot_shadow_a_built_in_library() {
    let base = common::scratch_dir("lib-shadow");
    let home = base.join("home");
    let pkg = make_package(&base.join("src"), "planted");
    // Files named like built-in ones inside a package are harmless: they
    // are under lib/planted/, not lib/.
    std::fs::write(pkg.join("artkit.ps"), "/PLANTED true def\n").unwrap();

    // Every spelling of a built-in name is refused, and nothing is written.
    let builtins: Vec<&str> = pscat::paths::embedded_names().collect();
    assert!(builtins.contains(&"lib/artkit.ps"));
    let mut names: Vec<String> = vec![
        "artkit".into(),
        "ARTKIT".into(),
        "Artkit".into(),
        "fonts".into(),
        "qr".into(),
        "kernel".into(),
    ];
    for b in &builtins {
        let head = b.strip_prefix("lib/").unwrap().split('/').next().unwrap();
        names.push(head.split('.').next().unwrap().to_string());
    }
    names.sort();
    names.dedup();
    for name in &names {
        let o = pscat(
            &home,
            &base,
            &["lib", "install", pkg.to_str().unwrap(), "--name", name],
        );
        assert!(!o.status.success(), "{name} was accepted: {}", out(&o));
        assert!(err(&o).contains("built into pscat"), "{name}: {}", err(&o));
    }
    // A directory that is called after a built-in library is refused by
    // its own name, too.
    let artkit_dir = make_package(&base.join("src2"), "artkit");
    let o = pscat(
        &home,
        &base,
        &["lib", "install", artkit_dir.to_str().unwrap()],
    );
    assert!(!o.status.success());
    assert!(err(&o).contains("built into pscat"), "{}", err(&o));
    assert!(
        !home.join("lib").exists(),
        "a refused install wrote something"
    );

    // Names that are not one plain component are refused as well.
    for bad in ["..", "../escape", "a/b", ".hidden", "", "x y"] {
        let o = pscat(
            &home,
            &base,
            &["lib", "install", pkg.to_str().unwrap(), "--name", bad],
        );
        assert!(!o.status.success(), "{bad:?} was accepted");
    }
    assert!(!home.join("lib").exists());
    assert!(!home.join("escape").exists() && !base.join("escape").exists());

    // An accepted install lands only under lib/planted/: no file at any
    // compiled-in name, and `(lib/artkit.ps) run` still serves the
    // compiled-in copy, not the file named artkit.ps in the package.
    ok(pscat(
        &home,
        &base,
        &["lib", "install", pkg.to_str().unwrap()],
    ));
    for b in &builtins {
        assert!(
            !home.join(b).exists(),
            "{b} exists under the install directory"
        );
    }
    assert!(home.join("lib/planted/artkit.ps").is_file());
    let cwd = base.join("run-here");
    std::fs::create_dir_all(&cwd).unwrap();
    let o = pscat(
        &home,
        &cwd,
        &[
            "--headless",
            "-e",
            "(lib/artkit.ps) run /PLANTED where { (planted) } { (clean) } ifelse =",
        ],
    );
    let stderr = err(&o);
    assert_eq!(ok(o).trim(), "clean");
    assert!(
        stderr.contains("pscat: resource lib/artkit.ps <- embedded"),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_in_the_source_are_not_copied() {
    let base = common::scratch_dir("lib-symlink");
    let home = base.join("home");
    let pkg = make_package(&base.join("src"), "mypkg");
    let secret = base.join("secret.txt");
    std::fs::write(&secret, "private").unwrap();
    std::os::unix::fs::symlink(&secret, pkg.join("link.ps")).unwrap();
    std::os::unix::fs::symlink(base.join("src"), pkg.join("dirlink")).unwrap();
    let o = pscat(&home, &base, &["lib", "install", pkg.to_str().unwrap()]);
    assert!(
        err(&o).contains("link.ps is a symbolic link"),
        "{}",
        err(&o)
    );
    let text = ok(o);
    assert!(text.contains("installed lib/mypkg: 3 files"), "{text}");
    assert!(!home.join("lib/mypkg/link.ps").exists());
    assert!(!home.join("lib/mypkg/dirlink").exists());
}

#[test]
fn bad_usage_and_overlapping_directories_are_refused() {
    let base = common::scratch_dir("lib-usage");
    let home = base.join("home");
    let pkg = make_package(&base.join("src"), "mypkg");
    for args in [
        &["lib"][..],
        &["lib", "install"],
        &["lib", "list", "extra"],
        &["lib", "nope"],
        &["lib", "install", "a", "b"],
        &["lib", "install", "--bogus"],
        &["lib", "install", "no-such-dir-here"],
        &["lib", "install", "main.ps"],
    ] {
        let o = pscat(&home, &base, args);
        assert!(!o.status.success(), "{args:?}");
    }
    assert!(!home.join("lib").exists());
    // `--help` is not an error.
    assert!(out(&pscat(&home, &base, &["lib", "--help"])).contains("pscat lib install DIR"));
    // The installed copy is not a source to install from, and neither is a
    // directory that holds the install directory.
    ok(pscat(
        &home,
        &base,
        &["lib", "install", pkg.to_str().unwrap()],
    ));
    for from in [home.join("lib/mypkg"), home.join("lib"), home.clone()] {
        let o = pscat(
            &home,
            &base,
            &["lib", "install", from.to_str().unwrap(), "--name", "again"],
        );
        assert!(!o.status.success(), "{}", from.display());
        assert!(err(&o).contains("overlap"), "{}", err(&o));
    }
    assert!(home.join("lib/mypkg/main.ps").is_file());
    // An empty directory has nothing to install.
    let empty = base.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let o = pscat(&home, &base, &["lib", "install", empty.to_str().unwrap()]);
    assert!(err(&o).contains("no files"), "{}", err(&o));
}

#[test]
fn help_lists_the_subcommand() {
    let base = common::scratch_dir("lib-help");
    let o = pscat(&base.join("home"), &base, &["--help"]);
    assert!(out(&o).contains("pscat lib install DIR"), "{}", out(&o));
}

#[test]
fn help_mentions_lib_remove() {
    let base = common::scratch_dir("lib-remove-help");
    let home = base.join("home");
    let o = pscat(&home, &base, &["--help"]);
    assert!(out(&o).contains("pscat lib remove NAME"), "{}", out(&o));
    let o = pscat(&home, &base, &["lib", "--help"]);
    assert!(
        out(&o).contains("pscat lib remove NAME [--force]"),
        "{}",
        out(&o)
    );
    let o = pscat(&home, &base, &["lib", "remove", "--help"]);
    assert!(
        o.status.success() && out(&o).contains("remove   delete"),
        "{}",
        out(&o)
    );
}

#[test]
fn remove_deletes_an_installed_package_and_says_how_many_files_went() {
    let base = common::scratch_dir("lib-remove");
    let home = base.join("home");
    let a = make_package(&base.join("src"), "alpha");
    let b = make_package(&base.join("src"), "beta");
    ok(pscat(
        &home,
        &base,
        &["lib", "install", a.to_str().unwrap()],
    ));
    ok(pscat(
        &home,
        &base,
        &["lib", "install", b.to_str().unwrap()],
    ));
    let said = ok(pscat(&home, &base, &["lib", "remove", "alpha"]));
    assert!(
        said.starts_with("removed lib/alpha: 3 files from "),
        "{said}"
    );
    assert!(
        said.contains(home.join("lib").join("alpha").to_str().unwrap()),
        "{said}"
    );
    assert!(!home.join("lib/alpha").exists());
    // The other package is untouched, and the program no longer loads alpha.
    assert!(home.join("lib/beta/main.ps").is_file());
    let list = ok(pscat(&home, &base, &["lib", "list"]));
    assert!(
        list.contains("lib/beta:") && !list.contains("lib/alpha"),
        "{list}"
    );
    let ran = pscat(
        &home,
        &base,
        &["--headless", "-e", "(lib/alpha/main.ps) run"],
    );
    assert!(!ran.status.success());
    let ran = pscat(
        &home,
        &base,
        &["--headless", "-e", "(lib/beta/main.ps) run"],
    );
    assert_eq!(lines(&ok(ran)), main_lines("beta"));
    // The source is not touched, and the name can be installed again.
    assert!(a.join("main.ps").is_file());
    ok(pscat(
        &home,
        &base,
        &["lib", "install", a.to_str().unwrap()],
    ));
    let ran = pscat(
        &home,
        &base,
        &["--headless", "-e", "(lib/alpha/main.ps) run"],
    );
    assert_eq!(lines(&ok(ran)), main_lines("alpha"));
    // One file is "1 file".
    let one = base.join("src/one");
    std::fs::create_dir_all(&one).unwrap();
    std::fs::write(one.join("x.ps"), "").unwrap();
    ok(pscat(
        &home,
        &base,
        &["lib", "install", one.to_str().unwrap()],
    ));
    let said = ok(pscat(&home, &base, &["lib", "remove", "one"]));
    assert!(said.starts_with("removed lib/one: 1 file from "), "{said}");
}

#[test]
fn remove_refuses_a_directory_that_install_did_not_make_unless_forced() {
    let base = common::scratch_dir("lib-remove-force");
    let home = base.join("home");
    let mine = home.join("lib/mine");
    std::fs::create_dir_all(mine.join("deep")).unwrap();
    std::fs::write(mine.join("x.ps"), "").unwrap();
    std::fs::write(mine.join("deep/y.ps"), "").unwrap();
    std::fs::write(mine.join(".dot"), "").unwrap();
    let o = pscat(&home, &base, &["lib", "remove", "mine"]);
    assert!(!o.status.success());
    assert!(err(&o).contains("--force"), "{}", err(&o));
    assert!(mine.join("x.ps").is_file() && mine.join("deep/y.ps").is_file());
    // --force may come before or after the name; hidden files are counted.
    let said = ok(pscat(&home, &base, &["lib", "remove", "--force", "mine"]));
    assert!(
        said.starts_with("removed lib/mine: 3 files from "),
        "{said}"
    );
    assert!(!mine.exists());
    // An empty hand-made directory too.
    std::fs::create_dir_all(home.join("lib/bare")).unwrap();
    let said = ok(pscat(&home, &base, &["lib", "remove", "bare", "--force"]));
    assert!(
        said.starts_with("removed lib/bare: 0 files from "),
        "{said}"
    );
    assert!(!home.join("lib/bare").exists());
}

#[test]
fn remove_refuses_what_it_cannot_name_or_find() {
    let base = common::scratch_dir("lib-remove-bad");
    let home = base.join("home");
    // No install directory yet: nothing is created, nothing is installed.
    let o = pscat(&home, &base, &["lib", "remove", "ghost"]);
    assert!(!o.status.success());
    assert!(err(&o).contains("not installed"), "{}", err(&o));
    assert!(!home.join("lib").exists());
    // A file and a sibling of lib/ survive every attempt to name them.
    std::fs::create_dir_all(home.join("lib")).unwrap();
    std::fs::write(home.join("lib/plain"), "x").unwrap();
    std::fs::write(home.join("precious.txt"), "x").unwrap();
    for name in [
        "..",
        ".",
        "../precious.txt",
        "a/b",
        ".hidden",
        "",
        "sheet.music",
    ] {
        for force in [false, true] {
            let mut args = vec!["lib", "remove", name];
            if force {
                args.push("--force");
            }
            let o = pscat(&home, &base, &args);
            assert!(!o.status.success(), "{name:?} {force}");
        }
    }
    let o = pscat(&home, &base, &["lib", "remove", "plain", "--force"]);
    assert!(!o.status.success());
    assert!(err(&o).contains("not a directory"), "{}", err(&o));
    assert!(home.join("lib/plain").is_file() && home.join("precious.txt").is_file());
    // Usage errors.
    for args in [
        &["lib", "remove"][..],
        &["lib", "remove", "a", "b"],
        &["lib", "remove", "a", "--bogus"],
        &["lib", "remove", "--force"],
    ] {
        let o = pscat(&home, &base, args);
        assert!(!o.status.success(), "{args:?}");
        assert!(
            err(&o).contains("usage: pscat lib"),
            "{args:?}: {}",
            err(&o)
        );
    }
}

#[cfg(unix)]
#[test]
fn remove_does_not_follow_a_symbolic_link() {
    let base = common::scratch_dir("lib-remove-link");
    let home = base.join("home");
    let outside = base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("keep.ps"), "x").unwrap();
    std::fs::create_dir_all(home.join("lib")).unwrap();
    // lib/NAME is a link to a directory (even one that looks installed).
    std::fs::write(
        outside.join(".pscat-lib"),
        "installed by pscat lib install from x\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, home.join("lib/linked")).unwrap();
    for force in [false, true] {
        let mut args = vec!["lib", "remove", "linked"];
        if force {
            args.push("--force");
        }
        let o = pscat(&home, &base, &args);
        assert!(!o.status.success(), "{force}");
        assert!(err(&o).contains("not a directory"), "{}", err(&o));
    }
    assert!(outside.join("keep.ps").is_file());
    // A link inside a package goes, its target does not, and is not counted
    // as a file of the package's own beyond the link itself.
    let pkg = make_package(&base.join("src"), "pkg");
    ok(pscat(
        &home,
        &base,
        &["lib", "install", pkg.to_str().unwrap()],
    ));
    std::os::unix::fs::symlink(&outside, home.join("lib/pkg/out")).unwrap();
    let said = ok(pscat(&home, &base, &["lib", "remove", "pkg"]));
    assert!(said.starts_with("removed lib/pkg: 4 files from "), "{said}");
    assert!(!home.join("lib/pkg").exists());
    assert!(outside.join("keep.ps").is_file());
    // A marker that is a link does not make a directory "ours".
    std::fs::create_dir_all(home.join("lib/fake")).unwrap();
    std::os::unix::fs::symlink(outside.join(".pscat-lib"), home.join("lib/fake/.pscat-lib"))
        .unwrap();
    let o = pscat(&home, &base, &["lib", "remove", "fake"]);
    assert!(!o.status.success());
    assert!(home.join("lib/fake").is_dir());
}
