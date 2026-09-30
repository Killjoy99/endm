//! Formatting and file name helpers used all over the IDM style UI.

/// Formats a byte count the way the list view shows it, e.g. `12.4 MB`.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];

    if bytes < 1024 {
        return format!("{bytes} bytes");
    }

    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    format!("{value:.1} {}", UNITS[unit])
}

/// Formats a transfer rate, e.g. `1.2 MB/sec`. Unknown rates render as `-`.
pub fn format_rate(bytes_per_sec: f64) -> String {
    if !(bytes_per_sec > 0.0) {
        return "-".to_string();
    }

    const UNITS: [&str; 4] = ["bytes", "KB", "MB", "GB"];
    let mut value = bytes_per_sec;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    let text = if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };

    format!("{text} {}/sec", UNITS[unit])
}

/// Formats seconds as the `hh:mm:ss` value of the "Time Left" column.
pub fn format_eta(seconds: f64) -> String {
    if !(seconds > 0.0) || !seconds.is_finite() {
        return "Unknown".to_string();
    }

    let total = seconds.ceil() as u64;
    if total > 99 * 3600 {
        return "Unknown".to_string();
    }

    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

/// The file name IDM suggests for a URL (`.../setup.exe?token=1` -> `setup.exe`).
pub fn file_name_from_url(url: &str) -> String {
    let without_scheme = match url.split_once("://") {
        Some((_, rest)) => rest,
        None => url,
    };
    let path = without_scheme.split(['?', '#']).next().unwrap_or("");
    let raw = path
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("");

    let decoded = percent_decode(raw);
    let sanitized = sanitize_file_name(&decoded);

    if sanitized.is_empty() {
        "download".to_string()
    } else {
        sanitized
    }
}

/// The lower case extension of a file name without the dot (`"report.PDF"` -> `"pdf"`).
pub fn extension_of(file_name: &str) -> String {
    let name = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name);

    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// The file name part of a path (`"./downloads/report.pdf"` -> `"report.pdf"`).
pub fn file_name_of_path(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Joins a directory and a file name with a single separator.
pub fn join_path(directory: &str, file_name: &str) -> String {
    let directory = directory.trim();
    if directory.is_empty() {
        return file_name.to_string();
    }

    if directory.ends_with('/') || directory.ends_with('\\') {
        format!("{directory}{file_name}")
    } else {
        format!("{directory}/{file_name}")
    }
}

/// Picks a file name that is not used by `taken` yet: `report.pdf` -> `report (1).pdf`.
pub fn unique_file_name(directory: &str, file_name: &str, taken: &[String]) -> String {
    let candidate = join_path(directory, file_name);
    if !contains_path(taken, &candidate) {
        return candidate;
    }

    let (stem, extension) = match file_name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_string(), format!(".{extension}")),
        _ => (file_name.to_string(), String::new()),
    };

    for n in 1..10_000 {
        let candidate = join_path(directory, &format!("{stem} ({n}){extension}"));
        if !contains_path(taken, &candidate) {
            return candidate;
        }
    }

    candidate
}

fn contains_path(paths: &[String], candidate: &str) -> bool {
    paths
        .iter()
        .any(|path| path.eq_ignore_ascii_case(candidate))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(code) = u8::from_str_radix(hex, 16) {
                output.push(code as char);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index] as char);
        index += 1;
    }

    output
}

fn sanitize_file_name(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if (ch as u32) < 0x20 => '_',
            ch => ch,
        })
        .collect();

    cleaned.trim().trim_end_matches('.').trim().to_string()
}

/// Puts `user:password@` into a URL when the server wants authorization.
pub fn with_credentials(url: &str, user_name: &str, password: &str) -> String {
    if user_name.is_empty() {
        return url.to_string();
    }

    match url.split_once("://") {
        Some((scheme, rest)) => format!("{scheme}://{user_name}:{password}@{rest}"),
        None => url.to_string(),
    }
}

/// Seconds from now until the clock next shows `hour`:`minute`.
pub fn seconds_until(hour: u32, minute: u32) -> u64 {
    use chrono::{Local, Timelike};

    let now = Local::now();
    let now_seconds = (now.hour() * 3600 + now.minute() * 60 + now.second()) as u64;
    let target = (hour as u64 * 3600 + minute as u64 * 60) % 86_400;

    if target > now_seconds {
        target - now_seconds
    } else {
        86_400 - now_seconds + target
    }
}
