use std::process::ExitCode;

fn main() -> ExitCode {
    app::database::tls::choose_ring();
    app::paths::carry_over_dotsql();
    app::updates::versions::activate_on_start();
    let _tunnels = app::tunnel::CloseAtExit;
    let argv = std::env::args_os()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    app::application().run_from(app::argv::rewrite(argv))
}
