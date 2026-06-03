//! Minimal `cargo xtask` runner. Real tasks (snapshot pinning, deny audits,
//! codegen) hang off the same dispatch.

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    match task.as_str() {
        "ci" => run(&[
            &["fmt", "--all", "--", "--check"],
            &["clippy", "--workspace", "--all-targets"],
            &["test", "--workspace"],
        ]),
        "fmt" => run(&[&["fmt", "--all"]]),
        "" | "help" | "-h" | "--help" => {
            println!("oncora xtask\n  tasks: ci | fmt | help");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown task: {other}\n  tasks: ci | fmt | help");
            ExitCode::FAILURE
        }
    }
}

fn run(steps: &[&[&str]]) -> ExitCode {
    for step in steps {
        eprintln!("> cargo {}", step.join(" "));
        let status = Command::new(env!("CARGO")).args(*step).status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => return ExitCode::from(s.code().unwrap_or(1) as u8),
            Err(e) => {
                eprintln!("failed to launch cargo: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
