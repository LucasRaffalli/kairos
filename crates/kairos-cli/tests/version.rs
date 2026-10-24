use std::process::Command;

#[test]
fn kairos_version_prints_name_and_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_kairos"))
        .arg("version")
        .output()
        .expect("le binaire kairos se lance");
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("kairos {}\n", env!("CARGO_PKG_VERSION"))
    );
}
