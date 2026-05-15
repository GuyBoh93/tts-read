use anyhow::Result;
use auto_launch::AutoLaunchBuilder;

pub fn sync(enabled: bool) -> Result<()> {
    let app_name = "TTS Read";
    let exe = std::env::current_exe()?;
    let exe_str = exe.to_str().unwrap_or_default();

    let launcher = AutoLaunchBuilder::new()
        .set_app_name(app_name)
        .set_app_path(exe_str)
        .build()?;

    if enabled {
        if !launcher.is_enabled()? {
            launcher.enable()?;
            tracing::info!("autostart enabled");
        }
    } else if launcher.is_enabled()? {
        launcher.disable()?;
        tracing::info!("autostart disabled");
    }

    Ok(())
}
