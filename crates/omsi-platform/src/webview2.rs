//! Microsoft's WebView2, which the desktop launcher draws its page with on Windows: part of
//! Windows 11 and of an updated Windows 10, missing on some older or trimmed systems. Where
//! the launcher's window could not open for want of it, the game's own launcher offers to
//! install it with Microsoft's own small installer (the "Evergreen Bootstrapper", which
//! downloads the runtime itself).

/// Microsoft's permanent link to the Evergreen Bootstrapper.
const BOOTSTRAPPER: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

/// Download the installer and run it, waiting until it has finished; Ok once it reported
/// success. Only Windows has WebView2.
pub fn install() -> anyhow::Result<()> {
    if !cfg!(windows) {
        anyhow::bail!("WebView2 is only needed on Windows");
    }
    let path = std::env::temp_dir().join("MicrosoftEdgeWebview2Setup.exe");
    let resp = crate::updater::agent().get(BOOTSTRAPPER).call()?;
    let mut out = std::fs::File::create(&path)?;
    std::io::copy(&mut resp.into_reader(), &mut out)?;
    drop(out);
    // (its own window shows the progress; Windows asks for the administrator's consent)
    let status = std::process::Command::new(&path).arg("/install").status()?;
    let _ = std::fs::remove_file(&path);
    if !status.success() {
        anyhow::bail!("the WebView2 installer ended with {status}");
    }
    Ok(())
}
