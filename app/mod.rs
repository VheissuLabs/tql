pub mod agent;
pub mod argv;
pub mod commands;
pub mod config_file;
pub mod console;
pub mod database;
pub mod dsn;
pub mod encrypter;
pub mod live;
pub mod mcp;
pub mod opener;
pub mod paths;
pub mod settings;
pub mod store;
pub mod store_writer;
pub mod tableplus;
pub mod tag;
pub mod terminal;
pub mod tui;
pub mod tunnel;
pub mod updates;

pub fn application() -> console::Application {
    let application = console::Application::new("Tql", concat!("v", env!("CARGO_PKG_VERSION")))
        .config_source("app", include_str!("../config/app.toml"))
        .config_source("commands", include_str!("../config/commands.toml"))
        .command(commands::browse::Browse)
        .command(commands::config::Config)
        .command(commands::connections::Connections)
        .command(commands::describe::Describe)
        .command(commands::export::Export)
        .command(commands::mcp::McpStart)
        .command(commands::open::Open)
        .command(commands::query::Query)
        .command(commands::tables::Tables)
        .command(commands::tableplus::TablePlus)
        .command(commands::update::Update);

    if cfg!(debug_assertions) {
        return application.command(console::MakeCommand);
    }
    application
}
