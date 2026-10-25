//! Launch at login on macOS, as a per-user launch agent.
//!
//! `SMAppService` needs an app bundle, which arrives with packaging in M8; until then
//! a launch agent in `~/Library/LaunchAgents` starts the binary at login. The adapter
//! only writes or deletes the file built here.

use std::path::{Path, PathBuf};

/// The launch agent's label, also its file name.
pub const LABEL: &str = "io.github.dimastatz.fluentb";

/// Where the launch agent lives for the user whose home is `home`.
#[must_use]
pub fn agent_path(home: &Path) -> PathBuf {
    home.join("Library")
        .join("LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

/// A launch agent that starts `executable` once each time the user logs in.
#[must_use]
pub fn agent_plist(executable: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
        escape(&executable.to_string_lossy())
    )
}

/// Escapes text for an XML element.
fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&apos;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_lives_in_the_user_launch_agents() {
        assert_eq!(
            agent_path(Path::new("/Users/u")),
            Path::new("/Users/u/Library/LaunchAgents/io.github.dimastatz.fluentb.plist")
        );
    }

    #[test]
    fn plist_runs_the_executable_at_login() {
        let plist = agent_plist(Path::new("/Applications/fluentb/fluentb"));

        assert!(plist.contains("<string>io.github.dimastatz.fluentb</string>"));
        assert!(plist.contains("<string>/Applications/fluentb/fluentb</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
    }

    #[test]
    fn plist_escapes_the_path() {
        let plist = agent_plist(Path::new("/Users/a&b/<it's \"here\">/fluentb"));

        assert!(plist.contains(
            "<string>/Users/a&amp;b/&lt;it&apos;s &quot;here&quot;&gt;/fluentb</string>"
        ));
    }
}
