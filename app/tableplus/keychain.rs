use std::cell::RefCell;
use std::collections::HashMap;
use std::process::Command;

pub const SERVICE: &str = "com.tableplus.TablePlus";

struct Fake {
    stored: HashMap<String, String>,
    asked: Vec<String>,
}

thread_local! {
    static FAKE: RefCell<Option<Fake>> = const { RefCell::new(None) };
}

pub fn fake(stored: &[(&str, &str)]) {
    FAKE.with_borrow_mut(|fake| {
        *fake = Some(Fake {
            stored: stored
                .iter()
                .map(|(account, password)| (account.to_string(), password.to_string()))
                .collect(),
            asked: Vec::new(),
        });
    });
}

pub fn asked() -> Vec<String> {
    FAKE.with_borrow(|fake| {
        fake.as_ref()
            .map(|fake| fake.asked.clone())
            .unwrap_or_default()
    })
}

pub fn password(account: &str) -> Option<String> {
    let faked = FAKE.with_borrow_mut(|fake| {
        fake.as_mut().map(|fake| {
            fake.asked.push(account.to_string());
            fake.stored.get(account).cloned()
        })
    });
    if let Some(answer) = faked {
        return answer;
    }
    if !cfg!(target_os = "macos") {
        return None;
    }

    let output = Command::new("security")
        .args(["find-generic-password", "-s", SERVICE, "-a", account, "-w"])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let read = String::from_utf8_lossy(&output.stdout);
    let password = read.trim_end_matches('\n');
    (!password.is_empty()).then(|| password.to_string())
}
