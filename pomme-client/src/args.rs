use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "pomme", about = "Minecraft client")]
pub struct LaunchArgs {
    #[arg(long)]
    pub version: Option<String>,

    #[arg(long)]
    pub username: Option<String>,

    #[arg(long)]
    pub uuid: Option<String>,

    #[arg(long)]
    pub access_token: Option<String>,

    #[arg(long)]
    pub launch_token: Option<String>,

    #[arg(long)]
    pub assets_dir: Option<String>,

    #[arg(long)]
    pub versions_dir: Option<String>,

    #[arg(long)]
    pub game_dir: Option<String>,

    #[arg(long)]
    pub quick_access_multiplayer: Option<String>,

    /// Run one unattended FPS benchmark after joining and stabilizing.
    #[arg(
        long,
        requires = "quick_access_multiplayer",
        conflicts_with = "render_probe_root"
    )]
    pub auto_fps_benchmark: bool,

    /// Launcher-generated nonce; unattended runs only.
    #[arg(long, requires = "auto_fps_benchmark", value_parser = parse_run_id)]
    pub auto_fps_run_id: Option<String>,

    /// Dedicated render probe request/results directory (opt-in).
    #[arg(long)]
    pub render_probe_root: Option<std::path::PathBuf>,
}

fn parse_run_id(s: &str) -> Result<String, &'static str> {
    if s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(s.to_ascii_lowercase())
    } else {
        Err("invalid auto FPS run ID")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_fps_requires_server() {
        assert!(LaunchArgs::try_parse_from(["pomme", "--auto-fps-benchmark"]).is_err());
        assert!(
            LaunchArgs::try_parse_from([
                "pomme",
                "--auto-fps-benchmark",
                "--quick-access-multiplayer",
                "example.org",
            ])
            .is_ok()
        );
    }
}
