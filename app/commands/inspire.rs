use std::time::{SystemTime, UNIX_EPOCH};

use zerorust::prelude::*;

const QUOTES: [&str; 4] = [
    "Simplicity is the ultimate sophistication. - Leonardo da Vinci",
    "Well begun is half done. - Aristotle",
    "Make it work, make it right, make it fast. - Kent Beck",
    "Very little is needed to make a happy life. - Marcus Aurelius",
];

#[command("inspire {name=Artisan}", description = "Display an inspiring quote")]
fn inspire(cmd: &mut Cmd) {
    let name = cmd.argument("name");
    cmd.info(format!("Simplicity is the essence of happiness, {name}."));

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos() as usize)
        .unwrap_or_default();
    cmd.comment(QUOTES[seed % QUOTES.len()]);
}
