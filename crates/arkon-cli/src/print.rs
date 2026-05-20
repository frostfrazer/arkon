/// ARKON terminal banner printed on every invocation.
pub fn banner() {
    if crate::json_output::is_enabled() { return; }
    println!();
    println!(
        "  \x1b[33m▲\x1b[0m  \x1b[1m\x1b[33mARKON\x1b[0m \x1b[2mv{} ({}·{})\x1b[0m  \
         \x1b[2mAutomated Runtime & Kernel Orchestration Node\x1b[0m",
        env!("CARGO_PKG_VERSION"),
        option_env!("ARKON_GIT_SHA").unwrap_or("dev"),
        option_env!("ARKON_BUILD_DATE").unwrap_or("local"),
    );
    println!();
}

pub fn success(msg: &str) {
    if crate::json_output::is_enabled() { return; }
    println!("  \x1b[32m✓\x1b[0m  {msg}");
}

pub fn info(msg: &str) {
    if crate::json_output::is_enabled() { return; }
    println!("  \x1b[2m›\x1b[0m  {msg}");
}

pub fn warn(msg: &str) {
    println!("  \x1b[33m⚠\x1b[0m  {msg}");
}

pub fn error(msg: &str) {
    eprintln!("  \x1b[31m✗\x1b[0m  {msg}");
}

pub fn step(label: &str, detail: &str) {
    println!("  \x1b[2m{label:12}\x1b[0m  {detail}");
}

pub fn separator() {
    println!("  \x1b[2m{}\x1b[0m", "─".repeat(52));
}

pub fn url(label: &str, url: &str) {
    println!("  \x1b[2m{label:12}\x1b[0m  \x1b[4m\x1b[36m{url}\x1b[0m");
}
