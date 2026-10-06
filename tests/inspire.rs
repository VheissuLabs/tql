#[test]
fn inspire_command_greets_and_quotes() {
    let result = app::application().run_with(["inspire", "Taylor"], "");

    assert_eq!(result.code, 0);
    assert!(
        result
            .stdout
            .starts_with("Simplicity is the essence of happiness, Taylor.\n")
    );
    assert!(result.stdout.contains(" - "));
}
