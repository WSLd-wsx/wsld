//! Download progress for terminals and redirected logs.
use std::time::Duration;

pub(crate) struct Progress {
    terminal: bool,
    width: usize,
    active: bool,
}

impl Progress {
    pub(crate) fn new() -> Self {
        use std::io::IsTerminal;
        let terminal = std::io::stderr().is_terminal();
        Self { terminal, width: 0, active: false }
    }

    pub(crate) fn is_terminal(&self) -> bool { self.terminal }

    pub(crate) fn update(&mut self, compact: &str, text: &str) {
        use std::io::Write;
        if self.terminal {
            let padding = self.width.saturating_sub(compact.len());
            let mut stderr = std::io::stderr().lock();
            let _ = write!(stderr, "\r{compact}{:padding$}", "");
            let _ = stderr.flush();
            self.width = compact.len();
            self.active = true;
        } else {
            eprintln!("{text}");
        }
    }

    pub(crate) fn finish(&mut self) {
        if self.active { eprintln!(); self.active = false; self.width = 0; }
    }
}

impl Drop for Progress {
    fn drop(&mut self) { self.finish(); }
}

pub(crate) fn format_progress(
    name: &str,
    received: u64,
    total: u64,
    transferred: u64,
    elapsed: Duration,
) -> String {
    let mib = 1024.0 * 1024.0;
    let percent = if total == 0 {
        0.0
    } else {
        100.0 * received.min(total) as f64 / total as f64
    };
    let speed = if elapsed.is_zero() {
        0.0
    } else {
        transferred as f64 / elapsed.as_secs_f64()
    };
    let eta = if received >= total && total != 0 {
        "0s".to_owned()
    } else if speed > 0.0 {
        format!(
            "{}s",
            (total.saturating_sub(received) as f64 / speed).ceil() as u64
        )
    } else {
        "estimating".to_owned()
    };
    format!(
        "Downloading {name}: {percent:.1}% ({:.1}/{:.1} MiB), {:.2} MiB/s, ETA {eta}",
        received as f64 / mib,
        total as f64 / mib,
        speed / mib
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resumed_speed_excludes_previously_downloaded_bytes() {
        let m = 1024 * 1024;
        let text = format_progress("image", 60 * m, 100 * m, 10 * m, Duration::from_secs(2));
        assert!(text.contains("60.0% (60.0/100.0 MiB), 5.00 MiB/s, ETA 8s"));
        assert!(
            format_progress("image", 50 * m, 100 * m, 0, Duration::ZERO).contains("ETA estimating")
        );
        assert!(
            format_progress("image", 100 * m, 100 * m, 50 * m, Duration::from_secs(10))
                .contains("100.0%")
        );
        assert!(
            format_progress("image", 100 * m, 100 * m, 50 * m, Duration::from_secs(10))
                .ends_with("ETA 0s")
        );
    }
}
