use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use app::paths;
use app::store_writer::StoreWriter;
use app::updates::remembered::{CHECKED, INSTALLED, LATEST};
use app::updates::updater::asset;
use app::updates::{
    How, Installation, Releases, Remembered, Updater, Versions, check, installation, releases,
};
use sha2::{Digest, Sha256};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct Route {
    path: String,
    status: u16,
    body: Vec<u8>,
}

struct FakeReleases {
    base: String,
    requested: Arc<Mutex<Vec<String>>>,
}

impl FakeReleases {
    fn serve(routes: Vec<Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requested = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requested);
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let Some(path) = requested_path(&mut stream) else {
                    continue;
                };
                recorded.lock().unwrap().push(path.clone());
                let (status, body) = routes
                    .iter()
                    .find(|route| route.path == path)
                    .map(|route| (route.status, route.body.clone()))
                    .unwrap_or((404, Vec::new()));
                let head = format!(
                    "HTTP/1.1 {status} Fake\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        FakeReleases { base, requested }
    }

    fn releases(&self) -> Releases {
        Releases::at(
            "VheissuLabs/tql",
            &format!("{}/latest", self.base),
            &format!("{}/download", self.base),
        )
    }

    fn updater(&self) -> Updater {
        Updater::new(self.releases())
    }

    fn downloaded(&self) -> bool {
        self.requested
            .lock()
            .unwrap()
            .iter()
            .any(|path| path.starts_with("/download/"))
    }
}

fn requested_path(stream: &mut std::net::TcpStream) -> Option<String> {
    let mut received = Vec::new();
    let mut buffer = [0u8; 1024];
    while !received.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        received.extend_from_slice(&buffer[..read]);
    }
    let request = String::from_utf8_lossy(&received).into_owned();
    request.split_whitespace().nth(1).map(String::from)
}

fn route(path: String, status: u16, body: impl Into<Vec<u8>>) -> Route {
    Route {
        path,
        status,
        body: body.into(),
    }
}

fn fake_binary(version: &str) -> String {
    format!("#!/bin/sh\necho 'Tql v{version}'\n")
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn signed(binary: &[u8]) -> String {
    format!("{}  {}\n", sha256(binary), asset())
}

fn release_of(version: &str, archive: Vec<u8>, sums: Option<String>) -> FakeReleases {
    let mut routes = vec![
        route(
            "/latest".to_string(),
            200,
            format!("{{\"tag_name\":\"v{version}\"}}"),
        ),
        route(format!("/download/v{version}/{}", asset()), 200, archive),
    ];
    if let Some(sums) = sums {
        routes.push(route(format!("/download/v{version}/SHA256SUMS"), 200, sums));
    }
    FakeReleases::serve(routes)
}

fn checked_release(version: &str, binary: &str) -> FakeReleases {
    let archive = binary.as_bytes().to_vec();
    let sums = signed(&archive);
    release_of(version, archive, Some(sums))
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "tql-update-{}-{}-{name}",
        std::process::id(),
        NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn installed_at(name: &str) -> PathBuf {
    let target = scratch(name).join("tql");
    fs::write(&target, fake_binary("0.7.0")).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    target
}

fn remembered(name: &str) -> Remembered {
    let store = scratch(name).join("tql.sqlite");
    StoreWriter::prepare(&store).unwrap();
    Remembered::new(&store)
}

fn leftover_downloads(versions: &Versions) -> Vec<String> {
    fs::read_dir(versions.directory())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".download-"))
        .collect()
}

fn staged_names(target: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(Versions::for_installed(target).directory())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("tql-"))
        .collect();
    names.sort();
    names
}

#[test]
fn stages_a_release_once_its_checksum_matches_without_touching_the_running_binary() {
    let target = installed_at("stage");
    let binary = fake_binary("0.8.0");

    checked_release("0.8.0", &binary)
        .updater()
        .install("0.8.0", &target)
        .unwrap();

    let versions = Versions::for_installed(&target);
    let staged = versions.file("0.8.0");
    assert_eq!(fs::read_to_string(&target).unwrap(), fake_binary("0.7.0"));
    assert_eq!(fs::read_to_string(&staged).unwrap(), binary);
    assert_eq!(
        fs::metadata(&staged).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(leftover_downloads(&versions).is_empty());
}

#[test]
fn switches_to_a_staged_version_through_a_symlink_so_a_running_tql_keeps_its_own_file() {
    let target = installed_at("switch");
    let binary = fake_binary("0.8.0");
    checked_release("0.8.0", &binary)
        .updater()
        .install("0.8.0", &target)
        .unwrap();

    assert!(Versions::for_installed(&target).activate(|| false));
    assert!(target.is_symlink());
    assert_eq!(
        fs::read_link(&target).unwrap(),
        Path::new(".tql-versions/tql-0.8.0")
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), binary);
    assert!(!Versions::for_installed(&target).activate(|| false));
}

#[test]
fn waits_to_turn_a_plain_binary_into_a_symlink_while_another_tql_runs_from_it() {
    let target = installed_at("wait");
    checked_release("0.8.0", &fake_binary("0.8.0"))
        .updater()
        .install("0.8.0", &target)
        .unwrap();

    assert!(!Versions::for_installed(&target).activate(|| true));
    assert!(!target.is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), fake_binary("0.7.0"));
    assert!(Versions::for_installed(&target).activate(|| false));
}

#[test]
fn switches_between_versions_once_it_is_a_symlink_even_with_another_tql_running() {
    let target = installed_at("between");

    for version in ["0.8.0", "0.8.1"] {
        checked_release(version, &fake_binary(version))
            .updater()
            .install(version, &target)
            .unwrap();
        Versions::for_installed(&target).activate(|| version == "0.8.1");
    }

    assert_eq!(
        fs::read_link(&target).unwrap(),
        Path::new(".tql-versions/tql-0.8.1")
    );
    assert_eq!(
        Versions::for_installed(&target.parent().unwrap().join(".tql-versions/tql-0.8.1")).link,
        target
    );
}

#[test]
fn keeps_only_the_last_three_versions() {
    let target = installed_at("kept");

    for version in ["0.8.0", "0.8.1", "0.10.0", "0.9.0"] {
        checked_release(version, &fake_binary(version))
            .updater()
            .install(version, &target)
            .unwrap();
    }

    assert_eq!(
        staged_names(&target),
        ["tql-0.10.0", "tql-0.8.1", "tql-0.9.0"]
    );
}

#[test]
fn refuses_a_download_that_does_not_match_its_checksum_and_leaves_the_old_one_alone() {
    let target = installed_at("mismatch");
    let archive = fake_binary("0.8.0").as_bytes().to_vec();
    let sums = format!("{}  {}\n", "0".repeat(64), asset());

    let failure = release_of("0.8.0", archive, Some(sums))
        .updater()
        .install("0.8.0", &target)
        .unwrap_err();

    assert!(failure.to_string().contains("checksum"), "{failure}");
    assert_eq!(fs::read_to_string(&target).unwrap(), fake_binary("0.7.0"));
    assert!(!Versions::for_installed(&target).file("0.8.0").exists());
}

#[test]
fn refuses_a_release_that_publishes_no_checksums() {
    let target = installed_at("unsigned");
    let archive = fake_binary("0.8.0").as_bytes().to_vec();

    let failure = release_of("0.8.0", archive, None)
        .updater()
        .install("0.8.0", &target)
        .unwrap_err();

    assert!(failure.to_string().contains("no checksums"), "{failure}");
    assert_eq!(fs::read_to_string(&target).unwrap(), fake_binary("0.7.0"));
}

#[test]
fn refuses_a_release_with_no_checksum_for_this_platform() {
    let target = installed_at("other-platform");
    let archive = fake_binary("0.8.0").as_bytes().to_vec();
    let sums = format!("{}  tql-plan9-mips\n", sha256(&archive));

    let failure = release_of("0.8.0", archive, Some(sums))
        .updater()
        .install("0.8.0", &target)
        .unwrap_err();

    assert_eq!(
        failure.to_string(),
        format!("v0.8.0 has no checksum for {}", asset())
    );
}

#[test]
fn refuses_a_binary_that_does_not_say_it_is_the_version_it_was_downloaded_as() {
    let target = installed_at("liar");

    let failure = checked_release("0.8.0", &fake_binary("0.6.0"))
        .updater()
        .install("0.8.0", &target)
        .unwrap_err();

    assert!(
        failure.to_string().contains("did not run as v0.8.0"),
        "{failure}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), fake_binary("0.7.0"));
    assert!(leftover_downloads(&Versions::for_installed(&target)).is_empty());
}

#[test]
fn says_to_use_sudo_when_it_cannot_write_beside_the_binary() {
    let target = installed_at("read-only");
    let folder = target.parent().unwrap().to_path_buf();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(folder.join("probe"), "").is_ok() {
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: this user can write into a read-only folder");
        return;
    }

    let failure = checked_release("0.8.0", &fake_binary("0.8.0"))
        .updater()
        .install("0.8.0", &target);
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        failure.unwrap_err().to_string(),
        format!(
            "{} is not writable; run tql update with sudo",
            folder.display()
        )
    );
}

#[test]
fn reads_the_version_from_its_own_line_not_from_whatever_else_the_download_prints() {
    let target = installed_at("noisy");
    let binary = "#!/bin/sh\necho 'Using something 8.4.26'\necho 'Tql v0.8.0'\n";

    checked_release("0.8.0", binary)
        .updater()
        .install("0.8.0", &target)
        .unwrap();

    assert_eq!(
        fs::read_to_string(Versions::for_installed(&target).file("0.8.0")).unwrap(),
        binary
    );
}

#[test]
fn checks_once_a_day_at_most() {
    let remembered = remembered("due");
    let now = check::now();

    assert!(check::due(true, &remembered, now));

    remembered.write(CHECKED, &(now - 2 * 3600).to_string());
    assert!(!check::due(true, &remembered, now));

    remembered.write(CHECKED, &(now - 2 * 86_400).to_string());
    assert!(check::due(true, &remembered, now));
    assert!(!check::due(false, &remembered, now));
    assert!(!check::enabled(false));
}

#[test]
fn updates_by_itself_when_it_can_and_says_a_restart_will_use_it() {
    let target = installed_at("by-itself");
    let binary = fake_binary("0.8.0");
    let release = checked_release("0.8.0", &binary);
    let remembered = remembered("by-itself-store");
    let installation = Installation::new(How::ByItself, &target);

    check::run(
        "0.7.0",
        &installation,
        &release.updater(),
        &remembered,
        true,
    );

    let releases = release.releases();
    assert_eq!(
        fs::read_to_string(Versions::for_installed(&target).file("0.8.0")).unwrap(),
        binary
    );
    assert_eq!(
        check::notice("0.7.0", &installation, &releases, &remembered).as_deref(),
        Some("tql 0.8.0 is ready · restart to use it")
    );
    assert_eq!(
        check::notice("0.8.0", &installation, &releases, &remembered),
        None
    );
    assert!(remembered.read(CHECKED).is_some());
}

#[test]
fn only_says_what_to_run_when_it_cannot_update_itself() {
    let release = checked_release("0.8.0", &fake_binary("0.8.0"));
    let remembered = remembered("homebrew");
    let installation = Installation::new(How::Homebrew, "/opt/homebrew/Cellar/tql/0.7.0/bin/tql");

    check::run(
        "0.7.0",
        &installation,
        &release.updater(),
        &remembered,
        true,
    );

    assert_eq!(
        check::notice("0.7.0", &installation, &release.releases(), &remembered).as_deref(),
        Some("tql 0.8.0 is out · brew upgrade tql")
    );
    assert!(!release.downloaded());
}

#[test]
fn only_says_a_new_version_is_out_when_automatic_updates_are_off() {
    let target = installed_at("manual");
    let release = checked_release("0.8.0", &fake_binary("0.8.0"));
    let remembered = remembered("manual-store");
    let installation = Installation::new(How::ByItself, &target);

    check::run(
        "0.7.0",
        &installation,
        &release.updater(),
        &remembered,
        false,
    );

    assert_eq!(fs::read_to_string(&target).unwrap(), fake_binary("0.7.0"));
    assert!(!release.downloaded());
    assert_eq!(
        check::notice("0.7.0", &installation, &release.releases(), &remembered).as_deref(),
        Some("tql 0.8.0 is out · tql update")
    );
}

#[test]
fn says_nothing_when_it_is_already_the_latest_or_there_are_no_releases_or_github_is_down() {
    let installation = Installation::new(How::ByItself, installed_at("quiet"));
    let remembered = remembered("quiet-store");

    let current = checked_release("0.7.0", &fake_binary("0.7.0"));
    let latest = check::run(
        "0.7.0",
        &installation,
        &current.updater(),
        &remembered,
        true,
    );
    assert_eq!(latest, None);
    assert_eq!(
        check::notice("0.7.0", &installation, &current.releases(), &remembered),
        None
    );

    for status in [404, 500] {
        let unavailable = FakeReleases::serve(vec![route("/latest".to_string(), status, "")]);
        let said = check::run(
            "0.7.0",
            &installation,
            &unavailable.updater(),
            &remembered,
            true,
        );
        assert_eq!(said, None);
        assert_eq!(
            check::notice("0.7.0", &installation, &unavailable.releases(), &remembered),
            None
        );
    }
    assert_eq!(remembered.read(LATEST).as_deref(), Some("0.7.0"));
    assert_eq!(remembered.read(INSTALLED), None);
}

#[test]
fn remembers_updates_under_the_keys_tql_has_always_used() {
    let remembered = remembered("apart");
    remembered.write(LATEST, "0.8.0");

    assert_eq!(remembered.read("updates.latest").as_deref(), Some("0.8.0"));
}

fn update(directory: &Path, arguments: &[&str]) -> (i32, String) {
    paths::use_config_directory(directory.join("tql"));
    let result = app::application().run_with(
        std::iter::once("update").chain(arguments.iter().copied()),
        "",
    );
    (result.code, result.stdout + &result.stderr)
}

fn current() -> String {
    app::updates::current_version()
}

#[test]
fn says_from_the_command_line_what_is_out_and_how_to_get_it_when_it_cannot_update_itself() {
    let release = checked_release("9.0.0", &fake_binary("9.0.0"));
    releases::use_source(release.releases());

    let (code, output) = update(&scratch("command-built"), &[]);

    assert_eq!(code, 0, "{output}");
    assert_eq!(
        output,
        format!(
            "tql 9.0.0 is out. You have {}; get it from github.com/VheissuLabs/tql/releases.\n",
            current()
        )
    );
    assert!(!release.downloaded());
}

#[test]
fn says_so_from_the_command_line_when_it_is_already_the_latest() {
    let release = checked_release(&current(), &fake_binary(&current()));
    releases::use_source(release.releases());

    let (code, output) = update(&scratch("command-latest"), &[]);

    assert_eq!(code, 0, "{output}");
    assert_eq!(output, format!("tql {} is the latest.\n", current()));
}

#[test]
fn says_clearly_when_nothing_has_been_released_yet() {
    let nothing = FakeReleases::serve(Vec::new());
    releases::use_source(nothing.releases());

    let (code, output) = update(&scratch("command-unreleased"), &[]);

    assert_eq!(code, 1);
    assert!(
        output.contains(
            "No tql release has been published at github.com/VheissuLabs/tql/releases yet."
        ),
        "{output}"
    );
}

#[test]
fn says_so_when_github_cannot_be_reached() {
    let down = FakeReleases::serve(vec![route("/latest".to_string(), 503, "")]);
    releases::use_source(down.releases());

    let (code, output) = update(&scratch("command-down"), &[]);

    assert_eq!(code, 1);
    assert!(
        output.contains("Could not reach GitHub to see what the latest release is."),
        "{output}"
    );
}

#[test]
fn updates_from_the_command_line_and_says_the_next_start_uses_it() {
    let target = installed_at("command-update");
    let release = checked_release("9.0.0", &fake_binary("9.0.0"));
    releases::use_source(release.releases());
    installation::pretend_running(Installation::new(How::ByItself, &target));
    let directory = scratch("command-update-config");

    let (code, output) = update(&directory, &[]);

    assert_eq!(code, 0, "{output}");
    assert_eq!(
        output,
        format!(
            "Updating tql {} to 9.0.0…\nDownloaded 9.0.0 and checked it against its published checksum. It takes over the next time you start tql.\n",
            current()
        )
    );
    assert!(!target.is_symlink());
    assert!(Versions::for_installed(&target).file("9.0.0").is_file());
    let store = Remembered::new(&directory.join("tql/tql.sqlite"));
    assert_eq!(store.read(INSTALLED).as_deref(), Some("9.0.0"));
}

#[test]
fn switches_at_once_from_the_command_line_when_it_is_already_a_symlink() {
    let target = installed_at("command-symlink");
    checked_release("8.0.0", &fake_binary("8.0.0"))
        .updater()
        .install("8.0.0", &target)
        .unwrap();
    assert!(Versions::for_installed(&target).activate(|| false));
    let release = checked_release("9.0.0", &fake_binary("9.0.0"));
    releases::use_source(release.releases());
    installation::pretend_running(Installation::new(How::ByItself, &target));

    let (code, output) = update(&scratch("command-symlink-config"), &[]);

    assert_eq!(code, 0, "{output}");
    assert!(
        output.ends_with(
            "Updated to 9.0.0, checked against its published checksum. The next tql you start is the new one.\n"
        ),
        "{output}"
    );
    assert_eq!(
        fs::read_link(&target).unwrap(),
        Path::new(".tql-versions/tql-9.0.0")
    );
}

#[test]
fn says_why_from_the_command_line_when_the_download_is_refused() {
    let target = installed_at("command-refused");
    let release = checked_release("9.0.0", &fake_binary("8.9.9"));
    releases::use_source(release.releases());
    installation::pretend_running(Installation::new(How::ByItself, &target));

    let (code, output) = update(&scratch("command-refused-config"), &[]);

    assert_eq!(code, 1);
    assert!(
        output.contains("The download for v9.0.0 did not run as v9.0.0, so it was not installed."),
        "{output}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), fake_binary("0.7.0"));
}

#[test]
fn checks_and_installs_quietly_in_the_background() {
    let target = installed_at("background");
    let release = checked_release("9.0.0", &fake_binary("9.0.0"));
    releases::use_source(release.releases());
    installation::pretend_running(Installation::new(How::ByItself, &target));
    let directory = scratch("background-config");

    let (code, output) = update(&directory, &["--background"]);

    assert_eq!((code, output.as_str()), (0, ""));
    assert!(Versions::for_installed(&target).file("9.0.0").is_file());
    let store = Remembered::new(&directory.join("tql/tql.sqlite"));
    assert!(store.read(CHECKED).is_some());
    assert_eq!(
        check::notice(
            &current(),
            &Installation::new(How::ByItself, &target),
            &release.releases(),
            &store
        )
        .as_deref(),
        Some("tql 9.0.0 is ready · restart to use it")
    );
}

#[test]
fn stays_quiet_in_the_background_when_nothing_has_been_released() {
    let nothing = FakeReleases::serve(Vec::new());
    releases::use_source(nothing.releases());

    let (code, output) = update(&scratch("background-unreleased"), &["--background"]);

    assert_eq!((code, output.as_str()), (0, ""));
}

#[test]
fn the_next_start_switches_to_the_staged_version_and_runs_it() {
    let folder = scratch("next-start");
    let versions = folder.join(".tql-versions");
    fs::create_dir_all(&versions).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_tql"),
        versions.join(format!("tql-{}", current())),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        format!(".tql-versions/tql-{}", current()),
        folder.join("tql"),
    )
    .unwrap();
    fs::write(versions.join("tql-9.9.9"), fake_binary("9.9.9")).unwrap();
    fs::set_permissions(
        versions.join("tql-9.9.9"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(versions.join("next"), "tql-9.9.9").unwrap();

    let started = std::process::Command::new(folder.join("tql"))
        .arg("--version")
        .output()
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&started.stdout), "Tql v9.9.9\n");
    assert_eq!(
        fs::read_link(folder.join("tql")).unwrap(),
        Path::new(".tql-versions/tql-9.9.9")
    );
    assert!(!versions.join("next").exists());
}
