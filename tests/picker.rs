use app::driver::Driver;
use app::store::Connection;
use app::terminal::Fake;
use app::tui::Picker;

fn connections() -> Vec<Connection> {
    vec![
        Connection {
            tag: Some("production".into()),
            ..Connection::sqlite("shop", "/tmp/shop.sqlite")
        },
        Connection {
            driver: Driver::Mysql,
            host: Some("db.test".into()),
            port: Some(3306),
            database: Some("blog".into()),
            username: Some("root".into()),
            ..Connection::sqlite("blog", "")
        },
    ]
}

#[test]
fn lists_connections_with_where_they_point() {
    let mut fake = Fake::new(Picker::new(connections(), None));

    let screen = fake.screen();
    assert!(screen.contains("CONNECTIONS"));
    assert!(screen.contains("sqlite:/tmp/shop.sqlite"));
    assert!(screen.contains("mysql://root@db.test:3306/blog"));
    assert!(screen.contains("production"));
    assert!(screen.contains("2 connections"));
}

#[test]
fn enter_opens_the_selected_connection() {
    let mut fake = Fake::new(Picker::new(connections(), None));

    fake.press("j\n");
    assert_eq!(
        fake.exit()
            .cloned()
            .flatten()
            .map(|connection| connection.name),
        Some("blog".into())
    );
}

#[test]
fn q_leaves_without_choosing() {
    let mut fake = Fake::new(Picker::new(connections(), None));

    fake.press("q");
    assert_eq!(fake.exit(), Some(&None));
}

#[test]
fn a_failed_connection_comes_back_as_a_notice() {
    let mut fake = Fake::new(Picker::new(
        connections(),
        Some("blog: connection refused".into()),
    ));

    assert!(fake.screen().contains("blog: connection refused"));
}

#[test]
fn an_empty_store_says_how_to_add_one() {
    let mut fake = Fake::new(Picker::new(Vec::new(), None));

    assert!(fake.screen().contains("No connections yet"));
}

#[test]
fn shows_the_icon_the_config_file_sets_for_a_driver() {
    let mut settings = app::settings::Settings::default();
    settings.set("icons.sqlite", &toml::Value::String("S".into()));
    app::settings::install(settings);
    let mut fake = Fake::new(Picker::new(connections(), None));

    let screen = fake.screen();
    app::settings::install(app::settings::Settings::default());

    assert!(screen.contains("S shop"), "{screen}");
}

#[test]
fn enter_on_an_empty_list_quits() {
    let mut fake = Fake::new(Picker::new(Vec::new(), None));

    fake.press("\r");

    assert!(fake.exited());
}
