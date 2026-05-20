mod mod_impl;

pub use mod_impl::detect;
pub use mod_impl::init;
pub use mod_impl::log;
pub use mod_impl::preview;
pub use mod_impl::promote;
pub use mod_impl::rollback;
pub use mod_impl::secrets;
pub use mod_impl::serve;
pub use mod_impl::status;
pub use mod_impl::adapter;

pub mod ship;

pub mod doctor {
    use arkon_core::error::Result;
    use std::path::Path;

    pub fn run(_root: &Path) -> Result<()> {
        println!();
        check("git",     cmd_ok("git"));
        check("ssh",     cmd_ok("ssh"));
        check("rsync",   cmd_ok("rsync"));
        check("docker",  cmd_ok("docker"));
        println!();
        crate::print::success("doctor check complete");
        Ok(())
    }

    fn cmd_ok(cmd: &str) -> bool {
        std::process::Command::new(cmd)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn check(name: &str, ok: bool) {
        if ok { println!("  \x1b[32m✓\x1b[0m  {name}"); }
        else  { println!("  \x1b[31m✗\x1b[0m  {name}  \x1b[2m(not found)\x1b[0m"); }
    }
}

pub mod cost {
    use arkon_core::{config::ArkonConfig, error::Result};
    use std::path::Path;

    pub async fn run(root: &Path, target: Option<&str>) -> Result<()> {
        let (config, _) = ArkonConfig::load(root)?;
        println!();
        for (name, _cfg) in &config.targets {
            if let Some(t) = target { if name != t { continue; } }
            println!("  \x1b[2m›\x1b[0m  \x1b[1m{:16}\x1b[0m  varies by usage", name);
        }
        println!();
        crate::print::info("run `arkon ship` to see actual costs after deploy");
        println!();
        Ok(())
    }
}
