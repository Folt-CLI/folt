use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_name(prefix: &str, separator: char) -> String {
    format!(
        "{prefix}{}{separator}{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests;
