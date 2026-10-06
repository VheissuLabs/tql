use std::process::ExitCode;

fn main() -> ExitCode {
    let argv = std::env::args_os()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    app::application().run_from(app::argv::rewrite(argv))
}
