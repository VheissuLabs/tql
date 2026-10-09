use crate::console::Cmd;
use crate::console::boxes::{self, cyan, dim, gray};

pub struct Progress {
    pub label: String,
    pub total: usize,
    pub progress: usize,
    pub hint: String,
    previous: Option<String>,
}

impl Progress {
    pub fn new(label: &str, total: usize, hint: &str) -> Self {
        Progress {
            label: label.to_string(),
            total,
            progress: 0,
            hint: hint.to_string(),
            previous: None,
        }
    }

    pub fn start(&mut self, cmd: &mut Cmd) {
        cmd.write("\x1b[?25l");
        self.render(cmd, false);
    }

    pub fn render(&mut self, cmd: &mut Cmd, finished: bool) {
        let frame = self.frame(columns(), finished);
        if self.previous.as_deref() == Some(frame.as_str()) {
            return;
        }
        match &self.previous {
            None => cmd.write(&frame),
            Some(previous) => {
                let height = previous.split('\n').count();
                let up = match height.saturating_sub(1) {
                    0 => String::new(),
                    lines => format!("\x1b[{lines}A"),
                };
                cmd.write(format!("\x1b[1G{up}\x1b[J{frame}"));
            }
        }
        self.previous = Some(frame);
    }

    pub fn finish(&mut self, cmd: &mut Cmd) {
        self.render(cmd, true);
        cmd.write("\x1b[?25h");
    }

    pub fn frame(&self, columns: usize, finished: bool) -> String {
        let room = columns.saturating_sub(6).max(1);
        let share = match self.total {
            0 => 0.0,
            total => self.progress as f64 / total as f64,
        };
        let filled = "█".repeat((share * boxes::min_width(columns) as f64).ceil() as usize);
        let label = boxes::truncate(&self.label, room);
        let title = match finished {
            true => dim(&label),
            false => cyan(&label),
        };
        let info = format!("{} / {}", grouped(self.progress), grouped(self.total));
        let mut frame = String::from("\n");
        for line in boxes::boxed(&title, &dim(&filled), &info, columns) {
            frame.push_str(&line);
            frame.push('\n');
        }
        if finished || self.hint.is_empty() {
            frame.push('\n');
        } else {
            frame.push_str(&gray(&format!("  {}", boxes::truncate(&self.hint, room))));
            frame.push('\n');
        }
        frame
    }
}

fn columns() -> usize {
    ratatui::crossterm::terminal::size()
        .map(|(columns, _)| columns as usize)
        .unwrap_or(80)
}

fn grouped(number: usize) -> String {
    let digits = number.to_string();
    let mut grouped = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_the_bar_in_a_box_with_the_count_and_the_table_under_it() {
        let mut progress = Progress::new("Dumping", 4, "connecting…");
        progress.progress = 1;

        let frame = boxes::strip(&progress.frame(80, false));
        let lines: Vec<&str> = frame.split('\n').collect();

        assert_eq!(lines[0], "");
        assert_eq!(lines[1], format!(" ┌ Dumping {}┐", "─".repeat(53)));
        assert_eq!(
            lines[2],
            format!(" │ {}{} │", "█".repeat(15), " ".repeat(45))
        );
        assert_eq!(lines[3], format!(" └{} 1 / 4 ┘", "─".repeat(55)));
        assert_eq!(lines[4], "  connecting…");
    }

    #[test]
    fn groups_thousands_as_the_count_grows() {
        assert_eq!(grouped(1234567), "1,234,567");
        assert_eq!(grouped(999), "999");
    }
}
