use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    let output = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let sha = output(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = output(&["status", "--porcelain", "--untracked-files=normal"])
        .is_some_and(|status| !status.is_empty());
    println!(
        "cargo:rustc-env=ECHORA_BUILD_SHA={sha}{}",
        if dirty { "+dirty" } else { "" }
    );
}
