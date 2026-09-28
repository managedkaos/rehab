use std::process::Command;

#[test]
fn prints_default_greeting() {
    let output = Command::new(env!("CARGO_BIN_EXE_rehab"))
        .output()
        .expect("failed to run rehab");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello, World\n");
}

#[test]
fn prints_named_greeting() {
    let output = Command::new(env!("CARGO_BIN_EXE_rehab"))
        .args(["--name", "Copilot"])
        .output()
        .expect("failed to run rehab");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello, Copilot\n");
}

#[test]
fn prints_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_rehab"))
        .arg("--help")
        .output()
        .expect("failed to run rehab");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: rehab [OPTIONS]"));
    assert!(stdout.contains("-n, --name <NAME>"));
    assert!(stdout.contains("-h, --help"));
}
