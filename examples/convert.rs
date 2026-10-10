use std::process::ExitCode;

fn main() -> ExitCode {
    let mut failed = false;
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("readable file");
        match into_md::convert(&bytes, &into_md::Limits::default()) {
            Ok(converted) => println!("===== {path} ({})\n{}\n", converted.format.as_str(), converted.markdown),
            Err(error) => {
                failed = true;
                eprintln!("===== {path}: {error}");
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
