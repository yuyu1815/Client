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

    /// Dedicated render probe request/results directory (opt-in).
    #[arg(long)]
    pub render_probe_root: Option<std::path::PathBuf>,
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
