use std::fs;
use std::path::PathBuf;

use app::driver::Driver;
use app::encrypter::Encrypter;
use app::store::{Connection, Store};
use app::store_writer::{Field, StoreWriter};
use app::terminal::Fake;
use app::tui::{Picker, Places};

const SAVE: &str = "\x13";

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tql-connections-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("home/.ssh")).unwrap();
        fs::create_dir_all(root.join("home/project/database")).unwrap();
        StoreWriter::prepare(&root.join("config/tql/tql.sqlite")).unwrap();
        Scratch { root }
    }

    fn store(&self) -> PathBuf {
        self.root.join("config/tql/tql.sqlite")
    }

    fn key(&self) -> PathBuf {
        self.root.join("config/tql/key")
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn writer(&self) -> StoreWriter {
        StoreWriter::open(&self.store(), &self.key()).unwrap()
    }

    fn add(&self, fields: &[(&'static str, Field)]) -> i64 {
        self.writer().create(fields).unwrap()
    }

    fn connections(&self) -> Vec<Connection> {
        Store::open(&self.store()).unwrap().connections().unwrap()
    }

    fn named(&self, name: &str) -> Option<Connection> {
        self.connections()
            .into_iter()
            .find(|connection| connection.name == name)
    }

    fn picker(&self) -> Fake<Picker> {
        Fake::sized(
            Picker::new(self.connections(), None)
                .with_store(&self.store(), &self.key())
                .with_places(Places {
                    home: self.home(),
                    cwd: self.home().join("project"),
                }),
            120,
            30,
        )
    }

    fn with_two(&self) -> Fake<Picker> {
        for (name, path) in [("first", "/tmp/a.sqlite"), ("second", "/tmp/b.sqlite")] {
            self.add(&sqlite(name, path));
        }
        self.picker()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn sqlite(name: &str, path: &str) -> Vec<(&'static str, Field)> {
    vec![
        ("name", Field::Text(name.into())),
        ("driver", Field::Text("sqlite".into())),
        ("database", Field::Text(path.into())),
    ]
}

fn mysql(name: &str, password: Option<&str>) -> Vec<(&'static str, Field)> {
    vec![
        ("name", Field::Text(name.into())),
        ("driver", Field::Text("mysql".into())),
        ("host", Field::Text("db.example.com".into())),
        ("port", Field::Number(3306)),
        ("database", Field::Text("shop".into())),
        ("username", Field::Text("alice".into())),
        (
            "password",
            password.map_or(Field::Null, |password| Field::Secret(password.into())),
        ),
    ]
}

fn line_with(screen: &str, needle: &str) -> String {
    screen
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_default()
        .to_string()
}

fn row_of(screen: &str, needle: &str) -> usize {
    screen
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no {needle} in\n{screen}"))
}

fn form_keys(fake: &Fake<Picker>) -> Vec<&'static str> {
    fake.app.form.as_ref().unwrap().keys()
}

fn move_to(fake: &mut Fake<Picker>, key: &str) {
    while fake.app.form.as_ref().unwrap().current_key() != key {
        fake.press("j");
    }
}

#[test]
fn edits_the_highlighted_connection() {
    let scratch = Scratch::new("edit-first");
    let mut fake = scratch.with_two();

    fake.press("e");

    let screen = fake.screen();
    assert!(screen.contains("EDIT CONNECTION"), "{screen}");
    assert!(line_with(&screen, "Name").contains("first"));
    assert!(!fake.exited());
}

#[test]
fn edits_the_one_the_cursor_moved_to() {
    let scratch = Scratch::new("edit-second");
    let mut fake = scratch.with_two();

    fake.press("je");

    assert!(line_with(&fake.screen(), "Name").contains("second"));
}

#[test]
fn clicking_a_connection_opens_the_one_under_the_pointer() {
    let scratch = Scratch::new("click-open");
    for (name, path) in [
        ("first", "/tmp/a.sqlite"),
        ("second", "/tmp/b.sqlite"),
        ("third", "/tmp/c.sqlite"),
    ] {
        scratch.add(&sqlite(name, path));
    }
    let mut settings = app::settings::Settings::default();
    settings.set("ui.mouse_row_offset", &toml::Value::Integer(0));
    app::settings::install(settings);
    let mut fake = scratch.picker();
    let screen = fake.screen();
    let row = screen
        .lines()
        .position(|line| line.contains("second"))
        .unwrap();

    fake.click(20, row as u16 + 1);

    assert_eq!(
        fake.exit()
            .cloned()
            .flatten()
            .map(|connection| connection.name),
        Some("second".into()),
        "{screen}"
    );
}

#[test]
fn ctrl_k_offers_the_commands_and_connections_of_this_screen() {
    let scratch = Scratch::new("palette");
    let mut fake = scratch.with_two();

    fake.press("\x0b");
    let screen = fake.screen();

    assert!(screen.contains("COMMANDS"), "{screen}");
    assert!(screen.contains("Add a connection"), "{screen}");
    assert!(screen.contains("second"), "{screen}");
    assert!(!screen.contains("Focus the rows"), "{screen}");

    fake.press("sec\n");

    assert_eq!(
        fake.exit()
            .cloned()
            .flatten()
            .map(|connection| connection.name),
        Some("second".into())
    );
}

#[test]
fn the_palette_runs_a_command_and_esc_closes_it() {
    let scratch = Scratch::new("palette-command");
    let mut fake = scratch.with_two();

    fake.press("\x0b\x1b");
    assert!(fake.app.palette.is_none());

    fake.press("\x0badd\n");
    assert!(fake.screen().contains("NEW CONNECTION"));
}

#[test]
fn a_browser_command_typed_here_says_where_it_works() {
    let scratch = Scratch::new("wrong-screen");
    let mut fake = scratch.with_two();

    fake.press(":sql\n");

    assert_eq!(
        fake.app.status.as_deref(),
        Some(":sql works once a connection is open")
    );
}

#[test]
fn still_opens_on_enter() {
    let scratch = Scratch::new("enter");
    let mut fake = scratch.with_two();

    fake.press("\n");

    assert_eq!(
        fake.exit()
            .cloned()
            .flatten()
            .map(|connection| connection.name),
        Some("first".into())
    );
}

#[test]
fn does_nothing_when_there_is_nothing_to_edit() {
    let scratch = Scratch::new("nothing");
    let mut fake = scratch.picker();

    fake.press("e");

    assert!(fake.app.form.is_none());
    assert!(!fake.exited());
    assert!(
        fake.screen()
            .contains("No connections yet. Press n to add one.")
    );
}

#[test]
fn offers_managing_connections_on_the_hotkey_bar() {
    let scratch = Scratch::new("bar");
    let mut fake = scratch.with_two();

    let screen = fake.screen();
    assert!(
        screen.contains("e Edit    n New    y/Y Yank db/DSN    d Delete    :w Confirm    :q Quit")
    );
    let lines: Vec<&str> = screen
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert!(lines[lines.len() - 2].contains("Move"));
    assert!(lines[lines.len() - 1].contains("2 connections"));
}

#[test]
fn closes_the_form_on_escape_without_saving() {
    let scratch = Scratch::new("escape");
    let mut fake = scratch.with_two();

    fake.press("e\nX\n\x1b");

    assert!(fake.app.form.is_none());
    assert!(scratch.named("first").is_some());
    assert_eq!(fake.app.status.as_deref(), Some("nothing changed"));
}

#[test]
fn saves_on_ctrl_s() {
    let scratch = Scratch::new("save");
    let mut fake = scratch.with_two();

    fake.press("e\n!\n").press(SAVE);

    assert!(fake.app.form.is_none());
    assert!(scratch.named("first!").is_some());
    assert!(fake.screen().contains("saved first!"));
}

#[test]
fn offers_only_the_fields_that_driver_has() {
    let scratch = Scratch::new("fields");
    scratch.add(&sqlite("lite", "/tmp/a.sqlite"));
    scratch.add(&mysql("my", Some("pw")));
    let mut fake = scratch.picker();

    fake.press("e");
    assert_eq!(
        form_keys(&fake),
        ["name", "database", "group_name", "tag", "read_only"]
    );

    fake.press("\x1b").press("je");
    assert_eq!(
        form_keys(&fake),
        [
            "name",
            "host",
            "port",
            "database",
            "username",
            "password",
            "ssl_mode",
            "over_ssh",
            "group_name",
            "tag",
            "read_only"
        ]
    );
}

#[test]
fn never_shows_the_password_back() {
    let scratch = Scratch::new("password");
    scratch.add(&mysql("shown", Some("hunter2")));
    let mut fake = scratch.picker();

    fake.press("e");

    let screen = fake.screen();
    assert!(line_with(&screen, "Password").contains("•••••••"));
    assert!(!line_with(&screen, "Password").contains("••••••••"));
    assert!(!screen.contains("hunter2"));
    assert!(line_with(&screen, "Host").contains("db.example.com"));
}

#[test]
fn shows_nothing_for_a_password_that_is_not_set() {
    let scratch = Scratch::new("no-password");
    scratch.add(&mysql("nopw", None));
    let mut fake = scratch.picker();

    fake.press("e");

    assert!(!line_with(&fake.screen(), "Password").contains('•'));
}

#[test]
fn refuses_to_save_a_name_that_is_already_taken() {
    let scratch = Scratch::new("taken");
    let mut fake = scratch.with_two();

    fake.press("e\n\x7f\x7f\x7f\x7f\x7fsecond\n").press(SAVE);

    assert!(
        fake.screen()
            .contains("Another connection is already called that.")
    );
    assert!(scratch.named("first").is_some());
}

#[test]
fn refuses_to_save_a_new_connection_under_a_taken_name() {
    let scratch = Scratch::new("taken-new");
    let mut fake = scratch.with_two();

    fake.press("nj\nfirst\nj\n/tmp/c.sqlite\n").press(SAVE);

    assert!(
        fake.screen()
            .contains("Another connection is already called that.")
    );
    assert_eq!(scratch.connections().len(), 2);
}

#[test]
fn refuses_to_save_an_empty_name() {
    let scratch = Scratch::new("empty-name");
    let mut fake = scratch.with_two();

    fake.press("e\n\x7f\x7f\x7f\x7f\x7f   \n").press(SAVE);

    assert!(fake.screen().contains("A name is required."));
}

#[test]
fn creates_a_connection_in_the_modal_rather_than_leaving() {
    let scratch = Scratch::new("new");
    let mut fake = scratch.with_two();

    fake.press("n");

    assert!(fake.app.form.as_ref().unwrap().creating);
    assert!(!fake.exited());
    assert!(fake.screen().contains("NEW CONNECTION"));
    assert_eq!(
        form_keys(&fake),
        [
            "driver",
            "name",
            "database",
            "group_name",
            "tag",
            "read_only"
        ]
    );
}

#[test]
fn colon_new_opens_the_same_form() {
    let scratch = Scratch::new("colon-new");
    let mut fake = scratch.with_two();

    fake.press(":new\n");

    assert!(fake.screen().contains("NEW CONNECTION"));
}

#[test]
fn cycles_the_driver_instead_of_typing_it() {
    let scratch = Scratch::new("cycle");
    let mut fake = scratch.with_two();

    fake.press("n");
    assert!(line_with(&fake.screen(), "Driver").contains("← sqlite →"));

    fake.press("\n");

    let form = fake.app.form.as_ref().unwrap();
    assert_eq!(form.driver(), Driver::Mysql);
    assert!(!form.editing_field());
    assert_eq!(
        form_keys(&fake),
        [
            "driver",
            "name",
            "host",
            "port",
            "database",
            "username",
            "password",
            "ssl_mode",
            "over_ssh",
            "group_name",
            "tag",
            "read_only"
        ]
    );
}

#[test]
fn fills_in_the_default_host_and_port_for_a_server_driver() {
    let scratch = Scratch::new("defaults");
    let mut fake = scratch.with_two();

    fake.press("nll");

    let form = fake.app.form.as_ref().unwrap();
    assert_eq!(form.driver(), Driver::Pgsql);
    assert_eq!(form.value("host"), "127.0.0.1");
    assert_eq!(form.value("port"), "5432");
}

#[test]
fn keeps_a_port_you_typed_when_the_driver_changes() {
    let scratch = Scratch::new("port");
    let mut fake = scratch.with_two();

    fake.press("nl");
    move_to(&mut fake, "port");
    fake.press("\n\x7f\x7f\x7f\x7f3307\n");
    fake.press("kkk").press("l");

    let form = fake.app.form.as_ref().unwrap();
    assert_eq!(form.driver(), Driver::Pgsql);
    assert_eq!(form.value("port"), "3307");
}

#[test]
fn saves_a_new_connection_and_selects_it() {
    let scratch = Scratch::new("added");
    let mut fake = scratch.with_two();

    fake.press("nj\nbrand new\nj\n/tmp/brand-new.sqlite\n")
        .press(SAVE);

    let created = scratch.named("brand new").unwrap();
    assert_eq!(created.driver, Driver::Sqlite);
    assert!(fake.app.form.is_none());
    assert_eq!(fake.app.status.as_deref(), Some("added brand new"));
    assert_eq!(fake.app.connections[fake.app.index].name, "brand new");
}

#[test]
fn will_not_save_a_sqlite_connection_with_no_path() {
    let scratch = Scratch::new("pathless");
    let mut fake = scratch.with_two();

    fake.press("nj\npathless\n").press(SAVE);

    assert!(fake.app.form.is_some());
    assert!(
        fake.screen()
            .contains("A path to the .sqlite file is required.")
    );
    assert!(scratch.named("pathless").is_none());
}

#[test]
fn saves_what_you_are_typing_without_committing_the_field_first() {
    let scratch = Scratch::new("mid-type");
    let mut fake = scratch.with_two();

    fake.press("njj\n/tmp/mid-type.sqlite\nk\ntyped then saved");
    assert!(fake.app.form.as_ref().unwrap().editing_field());

    fake.press(SAVE);

    assert!(fake.app.form.is_none());
    assert!(scratch.named("typed then saved").is_some());
}

#[test]
fn accepts_a_pasted_value_in_a_form_field() {
    let scratch = Scratch::new("paste");
    let mut fake = scratch.with_two();

    fake.press("nj\n").paste("Cloud - lunar");
    assert_eq!(fake.app.form.as_ref().unwrap().buffer(), "Cloud - lunar");

    fake.press("\n");
    assert_eq!(
        fake.app.form.as_ref().unwrap().value("name"),
        "Cloud - lunar"
    );
}

#[test]
fn flattens_a_multi_line_paste_into_a_single_line_field() {
    let scratch = Scratch::new("paste-lines");
    let mut fake = scratch.with_two();

    fake.press("nj\n").paste("first\r\nsecond\nthird");

    let form = fake.app.form.as_ref().unwrap();
    assert_eq!(form.buffer(), "first second third");
    assert!(form.editing_field());
}

#[test]
fn a_bracketed_paste_arrives_as_one_paste() {
    let scratch = Scratch::new("bracketed");
    let mut fake = scratch.with_two();

    fake.press("nj\n\x1b[200~first\nsecond\x1b[201~");

    assert_eq!(fake.app.form.as_ref().unwrap().buffer(), "first second");
}

#[test]
fn a_paste_with_nothing_to_type_into_does_nothing() {
    let scratch = Scratch::new("paste-nowhere");
    let mut fake = scratch.with_two();

    fake.paste("ndq");

    assert!(fake.app.form.is_none());
    assert!(fake.app.pending_deletes.is_empty());
    assert!(!fake.exited());
}

#[test]
fn moves_the_cursor_inside_a_field_instead_of_typing_the_key() {
    let scratch = Scratch::new("cursor");
    let mut fake = scratch.with_two();

    fake.press("e\n");
    let form = fake.app.form.as_ref().unwrap();
    assert_eq!((form.buffer().as_str(), form.cursor()), ("first", 5));

    fake.press("\x1b[D\x1b[D");
    assert_eq!(fake.app.form.as_ref().unwrap().cursor(), 3);

    fake.press("X");
    assert_eq!(fake.app.form.as_ref().unwrap().buffer(), "firXst");
}

#[test]
fn backspaces_at_the_cursor_not_only_at_the_end() {
    let scratch = Scratch::new("backspace");
    let mut fake = scratch.with_two();

    fake.press("e\n\x1b[D\x7f");

    assert_eq!(fake.app.form.as_ref().unwrap().buffer(), "firt");
}

#[test]
fn marks_a_connection_for_deletion_without_removing_it() {
    let scratch = Scratch::new("mark");
    let mut fake = scratch.with_two();

    fake.press("d");

    assert_eq!(fake.app.pending_deletes.len(), 1);
    assert_eq!(scratch.connections().len(), 2);
    assert!(
        fake.screen()
            .contains("1 marked for deletion · :w writes · u clears")
    );
    assert!(fake.highlighted().iter().any(|run| run.contains("first")));
}

#[test]
fn writes_marked_deletions_on_colon_w() {
    let scratch = Scratch::new("write");
    let mut fake = scratch.with_two();

    fake.press("d:w\n");

    assert!(scratch.named("first").is_none());
    assert_eq!(scratch.connections().len(), 1);
    assert!(fake.app.pending_deletes.is_empty());
    assert_eq!(fake.app.connections.len(), 1);
    assert!(fake.screen().contains("deleted 1 connection"));
}

#[test]
fn unmarks_a_connection_marked_twice() {
    let scratch = Scratch::new("unmark");
    let mut fake = scratch.with_two();

    fake.press("dkd");

    assert!(fake.app.pending_deletes.is_empty());
    assert!(fake.screen().contains("marks cleared"));
}

#[test]
fn u_clears_every_mark() {
    let scratch = Scratch::new("clear");
    let mut fake = scratch.with_two();

    fake.press("ddu");

    assert!(fake.app.pending_deletes.is_empty());
}

#[test]
fn drops_unwritten_marks_before_quitting() {
    let scratch = Scratch::new("drop");
    let mut fake = scratch.with_two();

    fake.press("dq");

    assert!(!fake.exited());
    assert!(fake.app.pending_deletes.is_empty());
    assert!(
        fake.screen()
            .contains("1 unwritten mark dropped, quit again to leave")
    );

    fake.press("q");
    assert_eq!(fake.exit(), Some(&None));
    assert_eq!(scratch.connections().len(), 2);
}

#[test]
fn groups_the_form_into_sections() {
    let scratch = Scratch::new("groups");
    let mut fake = scratch.with_two();

    fake.press("nl");
    move_to(&mut fake, "over_ssh");
    fake.press(" ");

    let screen = fake.screen();
    assert!(row_of(&screen, "SSL mode") - row_of(&screen, "Password") > 1);
    assert!(row_of(&screen, "Over SSH") - row_of(&screen, "SSL mode") > 1);
    assert!(screen.contains("agent or ~/.ssh/config"));
}

#[test]
fn turning_ssh_off_clears_what_it_hid() {
    let scratch = Scratch::new("ssh-off");
    let mut ssh = mysql("tunnelled", None);
    ssh.push(("ssh_host", Field::Text("bastion.test".into())));
    ssh.push(("ssh_user", Field::Text("deploy".into())));
    scratch.add(&ssh);
    let mut fake = scratch.picker();

    assert!(fake.screen().contains("ssh deploy@bastion.test"));
    fake.press("e");
    move_to(&mut fake, "over_ssh");
    fake.press(" ").press(SAVE);

    let saved = scratch.named("tunnelled").unwrap();
    assert_eq!(saved.ssh_host, None);
    assert_eq!(saved.ssh_user, None);
}

#[test]
fn picks_a_tag_from_a_list_that_shows_its_color() {
    let scratch = Scratch::new("tag");
    let mut fake = scratch.with_two();

    fake.press("n");
    move_to(&mut fake, "tag");
    fake.press("\n");

    let picker = fake.app.form.as_ref().unwrap().picker.as_ref().unwrap();
    assert_eq!(picker.title, "TAG");
    assert_eq!(
        picker.options,
        ["none", "production", "staging", "dev", "local"]
    );
    assert_eq!(picker.color_of("production"), Some("red"));
    assert_eq!(picker.color_of("local"), Some("green"));
    assert!(fake.screen().contains("● production"));
    assert!(fake.highlighted().iter().any(|run| run == "none"));

    fake.press("st\n");

    let form = fake.app.form.as_ref().unwrap();
    assert!(form.picker.is_none());
    assert_eq!(form.value("tag"), "staging");
    assert!(line_with(&fake.screen(), "Tag").contains("● staging"));
}

#[test]
fn picks_an_ssh_key_from_the_ones_in_home() {
    let scratch = Scratch::new("ssh-key");
    fs::write(
        scratch.home().join(".ssh/id_ed25519"),
        "-----BEGIN OPENSSH PRIVATE KEY-----\n",
    )
    .unwrap();
    fs::write(scratch.home().join(".ssh/id_ed25519.pub"), "ssh-ed25519").unwrap();
    let mut fake = scratch.with_two();

    fake.press("nl");
    move_to(&mut fake, "over_ssh");
    fake.press(" ");
    move_to(&mut fake, "ssh_key");
    fake.press("\n");

    let screen = fake.screen();
    assert!(screen.contains("SSH KEY"));
    assert!(screen.contains("~/.ssh/id_ed25519"));
    assert!(!screen.contains("id_ed25519.pub"));
    assert!(screen.contains("type a path…"));

    fake.press("\n");
    assert_eq!(
        fake.app.form.as_ref().unwrap().value("ssh_key"),
        "~/.ssh/id_ed25519"
    );
}

#[test]
fn type_a_path_starts_typing_instead() {
    let scratch = Scratch::new("type-path");
    let mut fake = scratch.with_two();

    fake.press("nl");
    move_to(&mut fake, "ssl_mode");
    fake.press("l");
    move_to(&mut fake, "ssl_ca");
    fake.press("\n\n/etc/ca.pem\n");

    assert_eq!(
        fake.app.form.as_ref().unwrap().value("ssl_ca"),
        "/etc/ca.pem"
    );
}

#[test]
fn dims_a_placeholder_but_not_a_real_value() {
    let scratch = Scratch::new("placeholder");
    let mut fake = scratch.with_two();

    fake.press("nl");

    let form = fake.app.form.as_ref().unwrap();
    assert!(form.is_placeholder("ssl_mode"));
    assert!(!form.is_placeholder("host"));
    assert!(!form.is_placeholder("name"));
    assert!(form.choices("driver").is_some());
    assert!(form.choices("read_only").is_some());
    assert!(form.choices("name").is_none());
}

#[test]
fn scrolls_a_long_path_while_it_is_typed() {
    let scratch = Scratch::new("long-path");
    let mut fake = scratch.with_two();

    fake.press("njj\n~/code/kmstools-lunar/database/database-for-the-lunar-project.sqlite");

    let screen = fake.screen();
    let row = line_with(&screen, "Path");
    assert!(row.contains("…"), "{row}");
    assert!(row.contains("lunar-project.sqlite"), "{row}");
    let top = line_with(&screen, "NEW CONNECTION");
    assert_eq!(
        row.chars()
            .nth(top.chars().position(|character| character == '╮').unwrap()),
        Some('│')
    );
}

#[test]
fn saves_a_relative_sqlite_path_as_an_absolute_one() {
    let scratch = Scratch::new("relative");
    let file = scratch.home().join("project/database/database.sqlite");
    fs::write(&file, "").unwrap();
    let mut fake = scratch.with_two();

    fake.press("nj\nrelative\nj\ndatabase/database.sqlite\n")
        .press(SAVE);

    assert_eq!(
        scratch.named("relative").unwrap().database,
        Some(fs::canonicalize(&file).unwrap().display().to_string())
    );
}

#[test]
fn saves_a_sqlite_path_under_home_with_home_in_place() {
    let scratch = Scratch::new("tilde");
    let file = scratch.home().join("lunar.sqlite");
    fs::write(&file, "").unwrap();
    let mut fake = scratch.with_two();

    fake.press("nj\nhome\nj\n~/lunar.sqlite\n").press(SAVE);

    assert_eq!(
        scratch.named("home").unwrap().database,
        Some(fs::canonicalize(&file).unwrap().display().to_string())
    );
}

#[test]
fn asks_for_a_bigger_window_on_the_connection_list_too() {
    let scratch = Scratch::new("small");
    let mut fake = Fake::sized(Picker::new(scratch.connections(), None), 40, 20);

    let screen = fake.screen();
    assert!(screen.contains("Make the window bigger"));
    assert!(screen.contains("tql needs 60×10"));
}

#[test]
fn a_saved_password_is_one_tql_can_read() {
    let scratch = Scratch::new("tql-reads");
    let mut fake = scratch.picker();

    fake.press("nl");
    move_to(&mut fake, "name");
    fake.press("\nshared\n");
    move_to(&mut fake, "password");
    fake.press("\ns3cret/pässword\n").press(SAVE);

    let saved = scratch.named("shared").unwrap();
    assert_eq!(
        Encrypter::from_file(&scratch.key())
            .unwrap()
            .decrypt_string(saved.password.as_deref().unwrap())
            .unwrap(),
        "s3cret/pässword"
    );

    let tql = PathBuf::from(
        std::env::var("TQL_PATH")
            .unwrap_or_else(|_| format!("{}/Code/tql", std::env::var("HOME").unwrap_or_default())),
    );
    if !tql.join("vendor/autoload.php").exists() {
        eprintln!("skipped the tql half: no tql checkout at {}", tql.display());
        return;
    }
    let script = format!(
        "require '{0}/vendor/autoload.php'; $app = require '{0}/bootstrap/app.php'; \
         $app->make(Illuminate\\Contracts\\Console\\Kernel::class)->bootstrap(); \
         $connection = App\\Models\\Connection::where('name', 'shared')->firstOrFail(); \
         echo json_encode([$connection->password, $connection->port, $connection->read_only, $connection->created_at->toDateTimeString()]);",
        tql.display()
    );
    let Ok(output) = std::process::Command::new("php")
        .args(["-r", &script])
        .env("XDG_CONFIG_HOME", scratch.root.join("config"))
        .env("HOME", scratch.home())
        .current_dir(&tql)
        .output()
    else {
        eprintln!("skipped the tql half: php is not installed");
        return;
    };
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let read: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(read[0], "s3cret/pässword");
    assert_eq!(read[1], 3306);
    assert_eq!(read[2], false);
    assert!(read[3].as_str().unwrap().starts_with("20"));
}

fn grouped(scratch: &Scratch) {
    for (name, group, used) in [
        ("blog", Some("Acme"), "2026-10-01 10:00:00"),
        ("nd-dev", Some("notarydash"), "2026-10-02 10:00:00"),
        ("scratch", None, "2026-10-04 10:00:00"),
        ("nd-prod", Some("notarydash"), "2026-10-03 10:00:00"),
    ] {
        let mut fields = sqlite(name, &format!("/tmp/{name}.sqlite"));
        if let Some(group) = group {
            fields.push(("group_name", Field::Text(group.into())));
        }
        fields.push(("last_used_at", Field::Text(used.into())));
        scratch.add(&fields);
    }
}

fn labels(fake: &mut Fake<Picker>) -> Vec<String> {
    let screen = fake.screen();
    let mut labels = Vec::new();
    for line in screen.lines().skip(3) {
        let Some(cell) = line
            .strip_prefix('│')
            .and_then(|line| line.split('│').next())
        else {
            continue;
        };
        let cell = cell.trim_start_matches([' ', '▸']);
        if let Some(header) = cell.strip_prefix('▼').or_else(|| cell.strip_prefix('▶')) {
            let name = header.trim().rsplit_once(" (").unwrap().0;
            labels.push(format!("# {name}"));
        } else if let Some((_, name)) = cell.split_once(' ')
            && !name.trim().is_empty()
        {
            labels.push(name.trim().to_string());
        }
    }
    labels
}

fn current_name(fake: &mut Fake<Picker>) -> String {
    let screen = fake.screen();
    let line = screen
        .lines()
        .find(|line| line.contains('▸'))
        .unwrap_or_default();
    line.split('│')
        .nth(1)
        .unwrap_or_default()
        .trim_start_matches([' ', '▸'])
        .trim_end()
        .to_string()
}

#[test]
fn lists_ungrouped_connections_first_then_each_group_under_a_header() {
    let scratch = Scratch::new("groups-listed");
    grouped(&scratch);
    let mut fake = scratch.picker();

    assert_eq!(
        labels(&mut fake),
        [
            "scratch",
            "# Acme",
            "blog",
            "# notarydash",
            "nd-prod",
            "nd-dev"
        ]
    );
    let screen = fake.screen();
    assert!(screen.contains("▼ notarydash (2)"), "{screen}");
    assert!(screen.contains("▼ Acme (1)"), "{screen}");
}

#[test]
fn folds_a_group_on_enter_over_its_header_and_unfolds_it_again() {
    let scratch = Scratch::new("groups-enter");
    grouped(&scratch);
    let mut fake = scratch.picker();

    fake.press("jjj\r");
    assert_eq!(
        labels(&mut fake),
        ["scratch", "# Acme", "blog", "# notarydash"]
    );
    assert!(fake.screen().contains("▶ notarydash (2)"));
    assert!(!fake.exited());

    fake.press("\r");
    assert_eq!(labels(&mut fake).len(), 6);
}

#[test]
fn folds_the_group_the_cursor_is_in_on_h_and_lands_on_its_header() {
    let scratch = Scratch::new("groups-h");
    grouped(&scratch);
    let mut fake = scratch.picker();

    fake.press("jjjjh");
    assert_eq!(fake.app.index, 3);
    assert_eq!(
        labels(&mut fake),
        ["scratch", "# Acme", "blog", "# notarydash"]
    );

    fake.press("l");
    assert_eq!(labels(&mut fake).len(), 6);

    fake.press(" ");
    assert_eq!(labels(&mut fake).len(), 4);
}

#[test]
fn remembers_folded_groups_the_next_time() {
    let scratch = Scratch::new("groups-remembered");
    grouped(&scratch);
    scratch.picker().press("j ");

    assert_eq!(
        labels(&mut scratch.picker()),
        ["scratch", "# Acme", "# notarydash", "nd-prod", "nd-dev"]
    );
}

#[test]
fn starts_on_the_connection_used_last_wherever_its_group_put_it() {
    let scratch = Scratch::new("groups-start");
    grouped(&scratch);
    scratch
        .writer()
        .update(
            scratch.named("blog").unwrap().id,
            &[("last_used_at", Field::Text("2026-10-08 10:00:00".into()))],
        )
        .unwrap();

    assert_eq!(
        current_name(&mut scratch.picker())
            .split_whitespace()
            .last(),
        Some("blog")
    );
}

#[test]
fn does_not_delete_or_yank_a_group_header() {
    let scratch = Scratch::new("groups-header-keys");
    grouped(&scratch);
    let clipboard = app::tui::clipboard::Recorded::default();
    let mut fake = Fake::sized(
        Picker::new(scratch.connections(), None)
            .with_store(&scratch.store(), &scratch.key())
            .with_clipboard(clipboard.clone()),
        120,
        30,
    );

    fake.press("jdyY");

    assert!(fake.app.form.is_none());
    assert!(fake.app.pending_deletes.is_empty());
    assert!(clipboard.copies().is_empty());
}

fn group_of(scratch: &Scratch, name: &str) -> Option<String> {
    scratch.named(name).unwrap().group_name
}

#[test]
fn renames_a_group_from_its_header_for_every_connection_in_it() {
    let scratch = Scratch::new("groups-rename");
    grouped(&scratch);
    let mut fake = scratch.picker();

    fake.press("jjje");
    assert!(fake.screen().contains("RENAME GROUP"));
    fake.press("-old\r");

    assert!(fake.app.renaming.is_none());
    assert_eq!(
        group_of(&scratch, "nd-dev").as_deref(),
        Some("notarydash-old")
    );
    assert_eq!(
        group_of(&scratch, "nd-prod").as_deref(),
        Some("notarydash-old")
    );
    assert_eq!(group_of(&scratch, "blog").as_deref(), Some("Acme"));
    assert_eq!(
        fake.app.status.as_deref(),
        Some("renamed notarydash to notarydash-old")
    );
    assert_eq!(
        labels(&mut fake),
        [
            "scratch",
            "# Acme",
            "blog",
            "# notarydash-old",
            "nd-prod",
            "nd-dev"
        ]
    );
    assert_eq!(fake.app.index, 3);
}

#[test]
fn renaming_a_group_to_one_in_use_merges_them() {
    let scratch = Scratch::new("groups-merge");
    grouped(&scratch);
    let mut fake = scratch.picker();

    fake.press("je\x7f\x7f\x7f\x7fnotarydash\r");

    assert_eq!(group_of(&scratch, "blog").as_deref(), Some("notarydash"));
    assert_eq!(
        fake.app.status.as_deref(),
        Some("moved Acme into notarydash")
    );
    assert!(fake.screen().contains("▼ notarydash (3)"));
}

#[test]
fn a_group_needs_a_name_and_escape_changes_nothing() {
    let scratch = Scratch::new("groups-rename-empty");
    grouped(&scratch);
    let mut fake = scratch.picker();

    fake.press("je\x7f\x7f\x7f\x7f\r");
    assert!(fake.app.renaming.is_some());
    assert!(fake.screen().contains("a group needs a name"));

    fake.press("\x1b");
    assert!(fake.app.renaming.is_none());
    assert_eq!(fake.app.status.as_deref(), Some("nothing changed"));
    assert_eq!(group_of(&scratch, "blog").as_deref(), Some("Acme"));
}

#[test]
fn a_folded_group_stays_folded_under_its_new_name() {
    let scratch = Scratch::new("groups-rename-folded");
    grouped(&scratch);
    let mut fake = scratch.picker();

    fake.press("jjj e\x7f\x7f\x7f\x7f\x7f\x7f\x7f\x7f\x7f\x7fND\r");

    assert_eq!(labels(&mut fake), ["scratch", "# Acme", "blog", "# ND"]);
    assert_eq!(
        labels(&mut scratch.picker()),
        ["scratch", "# Acme", "blog", "# ND"]
    );
}

#[test]
fn keeps_the_columns_where_they_are_when_a_group_folds() {
    let scratch = Scratch::new("groups-columns");
    grouped(&scratch);
    let mut fake = scratch.picker();
    let edges = |screen: &str| {
        let header = screen.lines().nth(1).unwrap_or_default().to_string();
        header
            .char_indices()
            .filter(|(_, character)| *character == '│')
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    };
    let before = edges(&fake.screen());

    fake.press("jjj ");

    assert_eq!(edges(&fake.screen()), before);
}

#[test]
fn picks_a_group_from_the_ones_in_use_or_makes_a_new_one_by_typing_it() {
    let scratch = Scratch::new("groups-form");
    grouped(&scratch);
    let mut fake = scratch.picker();

    fake.press("e");
    move_to(&mut fake, "group_name");
    fake.press("\r");
    let screen = fake.screen();
    assert!(screen.contains("GROUP"), "{screen}");
    assert_eq!(
        fake.app.form.as_ref().unwrap().choice_options(),
        ["none", "Acme", "notarydash"]
    );

    fake.press("side\r").press(SAVE);

    assert_eq!(
        scratch.named("scratch").unwrap().group_name.as_deref(),
        Some("side")
    );
    assert!(labels(&mut fake).contains(&"# side".to_string()));
    assert!(current_name(&mut fake).ends_with("scratch"));
}

#[test]
fn takes_a_connection_out_of_its_group_when_none_is_picked() {
    let scratch = Scratch::new("groups-none");
    grouped(&scratch);
    let mut fake = scratch.picker();

    fake.press("jje");
    move_to(&mut fake, "group_name");
    fake.press("\r").press("none\r").press(SAVE);

    assert_eq!(scratch.named("blog").unwrap().group_name, None);
    assert!(!labels(&mut fake).contains(&"# Acme".to_string()));
}

#[test]
fn saving_into_a_folded_group_opens_it_and_follows_the_connection() {
    let scratch = Scratch::new("groups-follow");
    grouped(&scratch);
    let mut fake = scratch.picker();

    fake.press("jjj ").press("kkk").press("e");
    move_to(&mut fake, "group_name");
    fake.press("\r").press("notarydash\r").press(SAVE);

    assert!(fake.screen().contains("▼ notarydash (3)"));
    assert!(current_name(&mut fake).ends_with("scratch"));
}

#[test]
fn says_on_the_hotkey_bar_how_to_fold_yank_and_delete() {
    let scratch = Scratch::new("groups-bar");
    let mut fake = scratch.with_two();
    let screen = fake.screen();

    assert!(screen.contains("space Fold"), "{screen}");
    assert!(screen.contains("y/Y Yank db/DSN"), "{screen}");
    assert!(screen.contains("d Delete"), "{screen}");
}

fn yanking(scratch: &Scratch) -> (Fake<Picker>, app::tui::clipboard::Recorded) {
    let clipboard = app::tui::clipboard::Recorded::default();
    let fake = Fake::sized(
        Picker::new(scratch.connections(), None)
            .with_store(&scratch.store(), &scratch.key())
            .with_clipboard(clipboard.clone()),
        120,
        30,
    );
    (fake, clipboard)
}

#[test]
fn yanks_the_database_name_on_y_and_the_connection_string_on_shift_y() {
    let scratch = Scratch::new("yank-mysql");
    scratch.add(&mysql("shop", Some("p@ss word")));
    let (mut fake, clipboard) = yanking(&scratch);

    fake.press("y");
    assert_eq!(fake.app.status.as_deref(), Some("yanked shop"));

    fake.press("Y");
    assert_eq!(fake.app.status.as_deref(), Some("yanked connection string"));
    assert_eq!(
        clipboard.copies(),
        [
            "shop",
            "mysql://alice:p%40ss%20word@db.example.com:3306/shop"
        ]
    );
}

#[test]
fn yanks_a_sqlite_connection_as_its_path() {
    let scratch = Scratch::new("yank-sqlite");
    scratch.add(&sqlite("local", "/tmp/a.sqlite"));
    let (mut fake, clipboard) = yanking(&scratch);

    fake.press("yY");

    assert_eq!(
        clipboard.copies(),
        ["/tmp/a.sqlite", "sqlite:///tmp/a.sqlite"]
    );
}

#[test]
fn says_so_when_there_is_no_database_name_to_yank() {
    let scratch = Scratch::new("yank-none");
    let mut fields = mysql("server", None);
    fields.retain(|(column, _)| *column != "database");
    scratch.add(&fields);
    let (mut fake, clipboard) = yanking(&scratch);

    fake.press("y");

    assert!(clipboard.copies().is_empty());
    assert_eq!(
        fake.app.status.as_deref(),
        Some("server has no database set, so it asks on connect")
    );
}

#[test]
fn says_when_a_yank_went_to_the_terminal_clipboard() {
    let scratch = Scratch::new("yank-terminal");
    scratch.add(&sqlite("local", "/tmp/a.sqlite"));
    let mut fake = Fake::sized(
        Picker::new(scratch.connections(), None).with_clipboard(
            app::tui::clipboard::Recorded::reaching(app::tui::clipboard::Reached::Terminal),
        ),
        120,
        30,
    );

    fake.press("y");

    assert_eq!(
        fake.app.status.as_deref(),
        Some("yanked /tmp/a.sqlite to the terminal clipboard")
    );
}

#[test]
fn lets_the_status_line_fade_back_to_the_connection_count() {
    let scratch = Scratch::new("yank-fade");
    scratch.add(&sqlite("local", "/tmp/a.sqlite"));
    let (mut fake, _) = yanking(&scratch);
    fake.press("y");
    let since = std::time::Instant::now();

    assert!(!fake.app.fade_status(since));
    assert_eq!(fake.app.status.as_deref(), Some("yanked /tmp/a.sqlite"));
    assert!(
        fake.app
            .fade_status(since + std::time::Duration::from_secs(60))
    );
    assert_eq!(fake.app.status, None);
}

#[test]
fn the_form_asks_each_new_driver_for_what_it_needs() {
    let mut form = app::tui::ConnectionForm::creating(Places {
        home: std::env::temp_dir(),
        cwd: std::env::temp_dir(),
    });
    let labels = |form: &app::tui::ConnectionForm| {
        form.fields()
            .iter()
            .map(|(_, label)| *label)
            .collect::<Vec<_>>()
    };
    let switch_to = |form: &mut app::tui::ConnectionForm, driver: Driver| {
        for _ in 0..Driver::ALL.len() {
            if form.driver() == driver {
                return;
            }
            form.cycle_driver(true);
        }
        panic!("the form never offered {driver}");
    };

    switch_to(&mut form, Driver::Duckdb);
    assert_eq!(
        labels(&form),
        ["Driver", "Name", "Path", "Group", "Tag", "Read only"]
    );

    switch_to(&mut form, Driver::Libsql);
    assert_eq!(
        labels(&form),
        [
            "Driver",
            "Name",
            "URL",
            "Auth token",
            "Group",
            "Tag",
            "Read only"
        ]
    );
    assert_eq!(form.value("host"), "");

    switch_to(&mut form, Driver::Clickhouse);
    assert!(labels(&form).contains(&"Host"));
    assert!(labels(&form).contains(&"SSL mode"));
    assert_eq!(form.value("port"), "8123");
    let at = form
        .keys()
        .iter()
        .position(|key| *key == "ssl_mode")
        .unwrap();
    form.move_by(at as isize);
    form.cycle(true);
    form.cycle(true);
    form.cycle(true);
    assert_eq!(form.value("ssl_mode"), "require");
    assert_eq!(form.value("port"), "8443");
}
