use std::process::Command;

#[test]
fn explicit_missing_config_fails_before_runtime_open() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("missing-filter.toml");
    let runtime = temp.path().join("missing-runtime");
    let output = Command::new(env!("CARGO_BIN_EXE_vader3p-steam"))
        .arg("--config")
        .arg(&config)
        .env("XDG_RUNTIME_DIR", &runtime)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success());
    assert!(stderr.contains("reading"), "{stderr}");
    assert!(stderr.contains(config.to_str().unwrap()), "{stderr}");
    assert!(!stderr.contains("opening runtime directory"), "{stderr}");
    assert!(!runtime.exists());
}

#[test]
fn absent_default_config_reaches_runtime_preflight() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = temp.path().join("missing-runtime");
    let output = Command::new(env!("CARGO_BIN_EXE_vader3p-steam"))
        .env("XDG_CONFIG_HOME", temp.path().join("missing-config"))
        .env("XDG_RUNTIME_DIR", &runtime)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success());
    assert!(stderr.contains("opening runtime directory"), "{stderr}");
    assert!(stderr.contains(runtime.to_str().unwrap()), "{stderr}");
    assert!(!runtime.exists());
}

#[test]
fn dangling_default_config_fails_before_runtime_open() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    let app = config_home.join("vader3p-steam");
    std::fs::create_dir_all(&app).unwrap();
    let config = app.join("config.toml");
    symlink(temp.path().join("missing-target"), &config).unwrap();
    let runtime = temp.path().join("missing-runtime");
    let output = Command::new(env!("CARGO_BIN_EXE_vader3p-steam"))
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_RUNTIME_DIR", &runtime)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success());
    assert!(stderr.contains("reading"), "{stderr}");
    assert!(stderr.contains(config.to_str().unwrap()), "{stderr}");
    assert!(!stderr.contains("opening runtime directory"), "{stderr}");
    assert!(!runtime.exists());
}
