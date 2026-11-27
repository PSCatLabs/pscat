//! Issue #487: no test may name a shared temp path.
//!
//! Agents run `cargo test` in several worktrees at once. A test that
//! writes `temp_dir()/some_fixed_name.png` then races its twin in the
//! other run (`read gs output: NotFound`). Every scratch path comes from
//! `common::scratch_dir`, which is unique per call and per process. This
//! test keeps it that way: it fails on any other route to the system temp
//! dir in a test file.

mod common;

#[test]
fn tests_take_scratch_paths_only_from_the_shared_helper() {
    // Built from pieces so this file does not match itself.
    let needles = [["temp", "_dir("].concat(), ["\"/", "tmp"].concat()];
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "scratch_paths.rs" {
            continue;
        }
        for (i, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if needles.iter().any(|n| line.contains(n.as_str())) {
                offenders.push(format!("tests/{name}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "use common::scratch_dir(tag) instead of the system temp dir:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn scratch_dirs_are_distinct_and_exist() {
    let a = common::scratch_dir("probe");
    let b = common::scratch_dir("probe");
    assert_ne!(a, b);
    assert!(a.is_dir() && b.is_dir());
    assert!(
        a.to_string_lossy()
            .contains(&std::process::id().to_string())
    );
    let _ = std::fs::remove_dir_all(&a);
    let _ = std::fs::remove_dir_all(&b);
}

// Issue #1481: the sweep of `pscat-test-<pid>` directories left by dead test
// processes. Each test sweeps a private base made with `scratch_dir`, never
// the real temp dir.
#[cfg(unix)]
mod sweep {
    use super::common;
    use std::path::Path;
    use std::time::Duration;

    /// Start the test binary itself with `args` and `env`, `TMPDIR` pointed
    /// at `tmp`; returns its pid after it has exited.
    fn run_self(tmp: &Path, args: &[&str], env: &[(&str, &str)]) -> u32 {
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args(args)
            .env("TMPDIR", tmp)
            .env_remove("PSCAT_KEEP_TEST_TMP")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = cmd.spawn().expect("spawn this test binary");
        let pid = child.id();
        let out = common::wait_with_deadline(child, common::default_deadline());
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        pid
    }

    /// A pid no process has: a child that has been started and waited for.
    fn dead_pid() -> u32 {
        run_self(&common::scratch_dir("sweep-dead"), &["--list"], &[])
    }

    /// Run only by the two tests below, in a child process: make a scratch
    /// directory with a file in it, then end like any test process.
    #[test]
    fn exit_probe() {
        if std::env::var_os("PSCAT_SCRATCH_EXIT_PROBE").is_none() {
            return;
        }
        let dir = common::scratch_dir("probe");
        std::fs::write(dir.join("f"), b"data").unwrap();
    }

    #[test]
    fn a_process_removes_its_scratch_directories_when_it_ends() {
        let tmp = common::scratch_dir("sweep-exit");
        let pid = run_self(
            &tmp,
            &["--exact", "sweep::exit_probe"],
            &[("PSCAT_SCRATCH_EXIT_PROBE", "1")],
        );
        assert!(
            !tmp.join(format!("pscat-test-{pid}")).exists(),
            "the child's directory is gone: {:?}",
            std::fs::read_dir(&tmp).unwrap().collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn keep_test_tmp_keeps_the_directories_and_marks_them_for_the_sweep() {
        let tmp = common::scratch_dir("sweep-keep");
        let pid = run_self(
            &tmp,
            &["--exact", "sweep::exit_probe"],
            &[
                ("PSCAT_SCRATCH_EXIT_PROBE", "1"),
                ("PSCAT_KEEP_TEST_TMP", "1"),
            ],
        );
        let root = tmp.join(format!("pscat-test-{pid}"));
        assert_eq!(std::fs::read(root.join("probe-0/f")).unwrap(), b"data");
        assert!(root.join(".keep").is_file());
        // The pid is dead, but the marker keeps a later sweep off it.
        assert_eq!(
            common::sweep_stale_scratch(&tmp, Duration::ZERO, 100, &|_| false),
            0
        );
        assert!(root.is_dir());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn fake_stale(base: &Path, name: &str) -> std::path::PathBuf {
        let dir = base.join(name).join("probe-0");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("big.bin"), b"x").unwrap();
        base.join(name)
    }

    /// A fake liveness check: only this process is alive.
    fn alive(pid: u32) -> bool {
        pid == std::process::id()
    }

    #[test]
    fn a_dead_process_directory_is_removed_and_live_young_and_kept_ones_stay() {
        let base = common::scratch_dir("sweep-basic");
        let dead = dead_pid();
        let stale = fake_stale(&base, &format!("pscat-test-{dead}"));
        let other_dead = fake_stale(&base, "pscat-test-4000001");
        let kept = fake_stale(&base, "pscat-test-4000002");
        std::fs::write(kept.join(".keep"), b"").unwrap();
        let mine = fake_stale(&base, &format!("pscat-test-{}", std::process::id()));
        // Other processes' pids: told to be dead, except 4000003 (alive).
        let alive_other = fake_stale(&base, "pscat-test-4000003");
        let ask = |pid: u32| pid == 4_000_003 || alive(pid);

        // Too young: nothing goes, whatever its pid.
        let n = common::sweep_stale_scratch(&base, Duration::from_secs(3600), 100, &ask);
        assert_eq!(n, 0);
        assert!(stale.is_dir() && other_dead.is_dir());

        // Old enough (min_age 0): the dead ones go, the rest stay.
        let n = common::sweep_stale_scratch(&base, Duration::ZERO, 100, &ask);
        assert_eq!(n, 2, "the dead child's and the fake with a dead pid");
        assert!(!stale.exists(), "a dead process's directory is removed");
        assert!(!other_dead.exists());
        assert!(kept.join("probe-0/big.bin").is_file(), ".keep is honoured");
        assert!(mine.is_dir(), "our own directory is never swept");
        assert!(alive_other.is_dir(), "a live process's directory stays");
        assert_eq!(
            common::sweep_stale_scratch(&base, Duration::ZERO, 100, &alive),
            1
        );
        assert!(!alive_other.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_real_liveness_check_tells_a_dead_process_from_a_live_one() {
        use std::os::unix::process::parent_id;
        let dead = dead_pid();
        assert!(!common::process_is_alive(dead), "a reaped child");
        assert!(common::process_is_alive(std::process::id()), "this process");
        assert!(common::process_is_alive(parent_id()), "the parent (cargo)");
        // Never a dead process: group addressing and out-of-range ids.
        assert!(common::process_is_alive(0));
        assert!(common::process_is_alive(u32::MAX));

        // A sweep with the real check removes the dead child's directory and
        // leaves the parent's.
        let base = common::scratch_dir("sweep-real");
        let gone = fake_stale(&base, &format!("pscat-test-{dead}"));
        let parent = fake_stale(&base, &format!("pscat-test-{}", parent_id()));
        let n = common::sweep_stale_scratch(&base, Duration::ZERO, 100, &common::process_is_alive);
        assert_eq!(n, 1);
        assert!(!gone.exists() && parent.is_dir());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_directory_that_is_too_young_is_kept_even_for_a_dead_pid() {
        let base = common::scratch_dir("sweep-young");
        let dir = fake_stale(&base, &format!("pscat-test-{}", dead_pid()));
        let hour = Duration::from_secs(3600);
        assert_eq!(common::sweep_stale_scratch(&base, hour, 100, &|_| false), 0);
        assert!(dir.is_dir());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn only_real_directories_named_pscat_test_digits_are_touched() {
        let base = common::scratch_dir("sweep-refuse");
        let never_alive = |_pid: u32| false;

        // A symlink named like one, pointing at a directory with a file.
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), b"keep me").unwrap();
        let link = base.join("pscat-test-1");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        // A dangling symlink too.
        let dangling = base.join("pscat-test-3");
        std::os::unix::fs::symlink(base.join("nowhere"), &dangling).unwrap();
        // A plain file, a non-numeric suffix, a bare prefix, other names.
        let file = base.join("pscat-test-2");
        std::fs::write(&file, b"a file").unwrap();
        let others: Vec<_> = [
            "pscat-test-abc",
            "pscat-test-12x",
            "pscat-test-",
            "pscat-test--5",
            "pscat-test-+7",
            "pscat-test-99999999999999999999",
            "pscat-testing-8",
            "xpscat-test-9",
            "other",
        ]
        .iter()
        .map(|n| fake_stale(&base, n))
        .collect();

        let n = common::sweep_stale_scratch(&base, Duration::ZERO, 100, &never_alive);
        assert_eq!(n, 0, "nothing here is a stale scratch directory");
        assert_eq!(std::fs::read(outside.join("sentinel")).unwrap(), b"keep me");
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert!(std::fs::symlink_metadata(&dangling).unwrap().is_symlink());
        assert_eq!(std::fs::read(&file).unwrap(), b"a file");
        for d in others {
            assert!(d.join("probe-0/big.bin").is_file(), "{}", d.display());
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_symlink_inside_a_stale_directory_is_removed_not_followed() {
        let base = common::scratch_dir("sweep-inner-link");
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), b"keep me").unwrap();
        let stale = fake_stale(&base, "pscat-test-5000001");
        std::os::unix::fs::symlink(&outside, stale.join("probe-0/link")).unwrap();
        let n = common::sweep_stale_scratch(&base, Duration::ZERO, 100, &|_| false);
        assert_eq!(n, 1);
        assert!(!stale.exists());
        assert_eq!(std::fs::read(outside.join("sentinel")).unwrap(), b"keep me");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn one_sweep_removes_at_most_the_cap() {
        let base = common::scratch_dir("sweep-cap");
        for i in 0..7 {
            fake_stale(&base, &format!("pscat-test-600000{i}"));
        }
        let left = |base: &Path| {
            std::fs::read_dir(base)
                .unwrap()
                .filter(|e| {
                    e.as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with("pscat-test-")
                })
                .count()
        };
        assert_eq!(
            common::sweep_stale_scratch(&base, Duration::ZERO, 3, &|_| false),
            3
        );
        assert_eq!(left(&base), 4);
        assert_eq!(
            common::sweep_stale_scratch(&base, Duration::ZERO, 100, &|_| false),
            4
        );
        assert_eq!(left(&base), 0);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_missing_base_is_not_an_error() {
        let gone = common::scratch_dir("sweep-missing").join("nope");
        assert_eq!(
            common::sweep_stale_scratch(&gone, Duration::ZERO, 10, &|_| false),
            0
        );
    }
}
