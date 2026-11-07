//! Contract for the one configuration gate (C-1a/b).
//!
//! `config validate`, `config apply` and `run` must agree on every failure
//! that can be derived from the configuration itself. Failures that depend on
//! the environment (bind, DNS, file permissions of the service user) are
//! deliberately outside this contract: the test proves consistency, not that
//! every runtime failure is detectable without running.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_eqproxy"))
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "eqproxy-gate-contract-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the test directory");
    dir
}

fn write_config(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(format!("{name}.toml"));
    std::fs::write(&path, contents).expect("write the test configuration");
    path
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn validate(config: &Path) -> Output {
    bin()
        .args(["config", "validate", "--config"])
        .arg(config)
        .output()
        .expect("run eqproxy config validate")
}

fn apply(state: &Path, config: &Path) -> Output {
    bin()
        .env("EQPROXY_STATE_DIR", state)
        .args(["config", "apply", "--config"])
        .arg(config)
        .output()
        .expect("run eqproxy config apply")
}

/// `run` blocks when it starts, so a failure case needs a bounded wait
/// instead of `output()`. A process that survives the deadline is killed and
/// reported as a contract failure, never left behind.
fn run_expect_failure(config: &Path) {
    let mut child = bin()
        .args(["run", "--config"])
        .arg(config)
        .args(["--metrics-interval", "1h"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn eqproxy run");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().expect("poll eqproxy run") {
            assert_eq!(
                status.code(),
                Some(1),
                "run must reject a configuration rejected by the gate: {config:?}"
            );
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run did not reject {config:?} within 10s");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn no_listeners() -> String {
    "schema_version = 1\n[backend.api]\nserver = \"127.0.0.1:9\"\n".to_owned()
}

fn missing_token_file(dir: &Path) -> String {
    format!(
        "schema_version = 1\n\
         [listener.web]\n\
         listen = \"127.0.0.1:0\"\n\
         backend = \"api\"\n\
         [backend.api]\n\
         server = \"127.0.0.1:9\"\n\
         [admin]\n\
         listen = \"127.0.0.1:0\"\n\
         token_file = \"{}\"\n",
        dir.join("missing.token").display()
    )
}

fn invalid_admin_listen() -> String {
    format!(
        "schema_version = 1\n\
         [listener.web]\n\
         listen = \"127.0.0.1:0\"\n\
         backend = \"api\"\n\
         [backend.api]\n\
         server = \"127.0.0.1:9\"\n\
         [admin]\n\
         listen = \"not-an-address\"\n\
         token = \"{}\"\n",
        "a".repeat(64)
    )
}

fn missing_access_log_parent(dir: &Path) -> String {
    format!(
        "schema_version = 1\n\
         [listener.web]\n\
         listen = \"127.0.0.1:0\"\n\
         backend = \"api\"\n\
         [backend.api]\n\
         server = \"127.0.0.1:9\"\n\
         [access_log]\n\
         path = \"{}\"\n",
        dir.join("missing-logs").join("access.log").display()
    )
}

fn valid_config() -> String {
    "schema_version = 1\n\
     shutdown_grace = \"1s\"\n\
     [listener.web]\n\
     listen = \"127.0.0.1:0\"\n\
     backend = \"api\"\n\
     [backend.api]\n\
     server = \"127.0.0.1:9\"\n"
        .to_owned()
}

#[test]
fn config_derived_failures_are_identical_across_validate_apply_and_run() {
    let dir = temp_dir("matrix");
    let cases = [
        ("no-listeners", no_listeners()),
        ("missing-token-file", missing_token_file(&dir)),
        ("invalid-admin-listen", invalid_admin_listen()),
        ("missing-access-log-parent", missing_access_log_parent(&dir)),
    ];

    for (name, source) in cases {
        let config = write_config(&dir, name, &source);

        let output = validate(&config);
        assert_eq!(
            output.status.code(),
            Some(1),
            "validate must reject {name}; stderr: {}",
            stderr(&output)
        );
        assert!(
            !stderr(&output).contains("warning: no [listener.*]"),
            "validate must not warn before the gate rejects {name}; stderr: {}",
            stderr(&output)
        );

        let state = dir.join(format!("state-{name}"));
        let output = apply(&state, &config);
        assert_eq!(
            output.status.code(),
            Some(1),
            "apply must reject {name}; stderr: {}",
            stderr(&output)
        );
        assert!(
            !state.join("current.toml").exists(),
            "a rejected apply must leave no current.toml for {name}"
        );

        run_expect_failure(&config);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn valid_config_passes_validate_apply_and_run() {
    let dir = temp_dir("valid");
    let state = dir.join("state");
    let config = write_config(&dir, "valid", &valid_config());

    let output = validate(&config);
    assert_eq!(
        output.status.code(),
        Some(0),
        "validate must accept the valid config; stderr: {}",
        stderr(&output)
    );

    let output = apply(&state, &config);
    assert_eq!(
        output.status.code(),
        Some(0),
        "apply must accept the valid config; stderr: {}",
        stderr(&output)
    );
    assert!(
        state.join("current.toml").exists(),
        "a successful apply must store current.toml"
    );

    let mut child = bin()
        .args(["run", "--config"])
        .arg(&config)
        .args(["--metrics-interval", "1h"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn eqproxy run");
    let mut stderr = BufReader::new(child.stderr.take().expect("stderr piped"));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = String::new();
    loop {
        assert!(
            Instant::now() < deadline,
            "run did not announce a listening socket"
        );
        line.clear();
        let read = stderr.read_line(&mut line).expect("read run stderr");
        assert!(read > 0, "run closed stderr without announcing a bind");
        if line.contains("listening on ") {
            break;
        }
    }

    let pid = child.id();
    let kill = Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .status()
        .expect("run kill -INT");
    assert!(kill.success(), "kill -INT must be accepted");

    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll run") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("run did not finish after SIGINT");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(status.code(), Some(0), "clean shutdown must exit with 0");

    let _ = std::fs::remove_dir_all(&dir);
}
