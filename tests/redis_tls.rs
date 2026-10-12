use app::driver::Driver;
use app::keystore;
use app::store::Connection;

#[test]
fn a_tls_redis_url_fails_to_connect_without_panicking_in_a_fresh_process() {
    let connection = Connection {
        name: "secure".into(),
        driver: Driver::Redis,
        host: Some("127.0.0.1".into()),
        port: Some(1),
        ssl_mode: Some("require".into()),
        ..Connection::sqlite("secure", "")
    };

    let failed = std::panic::catch_unwind(|| keystore::connect(&connection, None).err());

    let failed = failed.expect("connecting must not panic");
    assert!(failed.is_some());
}
