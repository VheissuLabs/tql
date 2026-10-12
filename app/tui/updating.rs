use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::paths;
use crate::updates::{self, Installation, Releases, Remembered, Updater, check};

pub const CHECKING: &str = "checking for a newer tql…";

enum Step {
    Installing(String),
    Done(Result<String, String>),
}

pub struct Updating {
    steps: Receiver<Step>,
}

impl Updating {
    pub fn start() -> Self {
        let current = updates::current_version();
        let installation = Installation::running();
        let updater = Updater::new(Releases::configured());
        let remembered = Remembered::new(&paths::store());
        let (sender, steps) = mpsc::channel();
        std::thread::spawn(move || {
            let installing = sender.clone();
            let said = check::by_hand(&current, &installation, &updater, &remembered, |line| {
                let _ = installing.send(Step::Installing(line.to_string()));
            });
            let _ = sender.send(Step::Done(said));
        });
        Updating { steps }
    }

    pub fn heard(&mut self) -> Option<(String, bool)> {
        match self.steps.try_recv() {
            Ok(Step::Installing(line)) => Some((line, false)),
            Ok(Step::Done(Ok(said))) => Some((said, true)),
            Ok(Step::Done(Err(failed))) => Some((failed, true)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(("The update stopped before it finished.".to_string(), true))
            }
        }
    }
}
