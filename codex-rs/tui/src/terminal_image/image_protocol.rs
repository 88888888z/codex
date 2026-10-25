use std::env;
use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use base64::Engine as _;
use base64::engine::general_purpose;
use codex_terminal_detection::Multiplexer;
use codex_terminal_detection::TerminalInfo;
use codex_terminal_detection::TerminalName;
use codex_terminal_detection::terminal_info;

const ESC: &str = "\x1b";
const ST: &str = "\x1b\\";
const KITTY_CHUNK_SIZE: usize = 4096;
const ITERM2_KITTY_MIN_VERSION: (u64, u64, u64) = (3, 6, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    KittyLocalFile,
    Sixel,
}

pub(crate) fn detect_image_protocol() -> Option<ImageProtocol> {
    if env::var_os("TMUX").is_some() || env::var_os("TMUX_PANE").is_some() {
        return None;
    }

    if env::var_os("ZELLIJ").is_some()
        || env::var_os("ZELLIJ_SESSION_NAME").is_some()
        || env::var_os("ZELLIJ_VERSION").is_some()
    {
        return None;
    }

    if env::var_os("KITTY_WINDOW_ID").is_some() {
        return Some(ImageProtocol::Kitty);
    }

    if env::var_os("WEZTERM_EXECUTABLE").is_some() || env::var_os("WEZTERM_VERSION").is_some() {
        return Some(ImageProtocol::Kitty);
    }

    image_protocol_for_terminal(&terminal_info())
}

fn image_protocol_for_terminal(info: &TerminalInfo) -> Option<ImageProtocol> {
    match info.multiplexer {
        Some(Multiplexer::Tmux { .. }) => {
            return None;
        }
        Some(Multiplexer::Zellij { .. }) => {
            return None;
        }
        None => {}
    }

    if supports_iterm2_kitty_graphics(info) {
        return Some(ImageProtocol::KittyLocalFile);
    }

    if is_iterm2_terminal(info) {
        return None;
    }

    if supports_kitty_graphics(info) {
        return Some(ImageProtocol::Kitty);
    }

    if supports_sixel(info) {
        return Some(ImageProtocol::Sixel);
    }

    None
}

fn supports_iterm2_kitty_graphics(info: &TerminalInfo) -> bool {
    is_iterm2_terminal(info)
        && version_is_at_least(
            info.version.as_deref(),
            /*minimum*/ ITERM2_KITTY_MIN_VERSION,
        )
}

fn is_iterm2_terminal(info: &TerminalInfo) -> bool {
    matches!(info.name, TerminalName::Iterm2)
        || terminal_field_contains(info.term_program.as_deref(), "iterm")
}

fn supports_kitty_graphics(info: &TerminalInfo) -> bool {
    matches!(
        info.name,
        TerminalName::Ghostty | TerminalName::Kitty | TerminalName::WezTerm
    ) || terminal_field_contains(info.term.as_deref(), "kitty")
        || terminal_field_contains(info.term.as_deref(), "ghostty")
        || terminal_field_contains(info.term.as_deref(), "wezterm")
        || terminal_field_contains(info.term_program.as_deref(), "kitty")
        || terminal_field_contains(info.term_program.as_deref(), "ghostty")
        || terminal_field_contains(info.term_program.as_deref(), "wezterm")
}

fn supports_sixel(info: &TerminalInfo) -> bool {
    matches!(info.name, TerminalName::WindowsTerminal)
        || terminal_field_contains(info.term.as_deref(), "sixel")
        || terminal_field_contains(info.term.as_deref(), "mlterm")
        || terminal_field_contains(info.term.as_deref(), "foot")
}

fn terminal_field_contains(value: Option<&str>, needle: &str) -> bool {
    value.is_some_and(|value| value.to_ascii_lowercase().contains(needle))
}

fn version_is_at_least(version: Option<&str>, minimum: (u64, u64, u64)) -> bool {
    parse_dotted_version(version).is_some_and(|version| version >= minimum)
}

fn parse_dotted_version(version: Option<&str>) -> Option<(u64, u64, u64)> {
    let version = version?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;

    if parts.next().is_some() {
        return None;
    }

    Some((major, minor, patch))
}

pub fn kitty_delete_image(image_id: u32) -> String {
    wrap_for_tmux_if_needed(&format!("{ESC}_Ga=d,d=I,i={image_id},q=2;{ST}"))
}

pub fn kitty_transmit_png_with_id(
    path: &Path,
    columns: u16,
    rows: u16,
    image_id: Option<u32>,
) -> Result<String> {
    let png = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let payload = general_purpose::STANDARD.encode(png);
    let chunks = payload
        .as_bytes()
        .chunks(KITTY_CHUNK_SIZE)
        .collect::<Vec<_>>();

    let mut command = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let chunk = std::str::from_utf8(chunk).context("base64 payload is not valid UTF-8")?;
        let has_more = index + 1 < chunks.len();
        let more_flag = u8::from(has_more);
        if index == 0 {
            let image_id = kitty_image_id_arg(image_id);
            command.push_str(&format!(
                "{ESC}_Ga=T,t=d,f=100,c={columns},r={rows},q=2{image_id},m={more_flag},C=1;{chunk}{ST}",
            ));
        } else {
            command.push_str(&format!("{ESC}_Gm={more_flag};{chunk}{ST}"));
        }
    }

    Ok(wrap_for_tmux_if_needed(&command))
}

pub fn kitty_transmit_png_file_with_id(
    path: &Path,
    columns: u16,
    rows: u16,
    image_id: Option<u32>,
) -> Result<String> {
    let path = path
        .canonicalize()
        .with_context(|| format!("canonicalize {}", path.display()))?;
    let payload = general_purpose::STANDARD.encode(path.to_string_lossy().as_bytes());
    let image_id = kitty_image_id_arg(image_id);
    let command =
        format!("{ESC}_Ga=T,t=f,f=100,c={columns},r={rows},q=2{image_id},C=1;{payload}{ST}");

    Ok(wrap_for_tmux_if_needed(&command))
}

fn kitty_image_id_arg(image_id: Option<u32>) -> String {
    image_id
        .map(|image_id| format!(",i={image_id}"))
        .unwrap_or_default()
}

fn wrap_for_tmux_if_needed(command: &str) -> String {
    if env::var_os("TMUX").is_none() {
        return command.to_string();
    }

    let escaped = command.replace(ESC, "\x1b\x1b");
    format!("{ESC}Ptmux;{escaped}{ST}")
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    struct EnvVarGuard {
        name: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn new(name: &'static str, value: Option<&str>) -> Self {
            let previous = env::var_os(name);
            match value {
                Some(value) => unsafe { env::set_var(name, value) },
                None => unsafe { env::remove_var(name) },
            }
            Self { name, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => unsafe { env::set_var(self.name, value) },
                None => unsafe { env::remove_var(self.name) },
            }
        }
    }

    #[test]
    #[serial]
    fn kitty_png_transmission_encodes_inline_data() {
        let _guard = EnvVarGuard::new("TMUX", /*value*/ None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.png");
        fs::write(&path, b"png").unwrap();

        let command = kitty_transmit_png_with_id(
            &path, /*columns*/ 4, /*rows*/ 3, /*image_id*/ None,
        )
        .unwrap();

        assert!(command.starts_with("\x1b_Ga=T,t=d,f=100,c=4,r=3,q=2,m=0,C=1;"));
        assert!(command.contains("cG5n"));
        assert!(command.ends_with("\x1b\\"));
    }

    #[test]
    #[serial]
    fn tmux_passthrough_wraps_and_escapes_control_sequence() {
        let _guard = EnvVarGuard::new("TMUX", Some("session"));
        assert_eq!(
            wrap_for_tmux_if_needed("\x1b_Gx;\x1b\\"),
            "\x1bPtmux;\x1b\x1b_Gx;\x1b\x1b\\\x1b\\"
        );
    }

    #[test]
    fn image_protocol_prefers_multiplexer_safety() {
        assert_eq!(
            image_protocol_for_terminal(&terminal_info_for_test(
                TerminalName::Ghostty,
                Some(Multiplexer::Tmux { version: None }),
                Some("Ghostty"),
                /*term*/ None,
            )),
            None
        );
        assert_eq!(
            image_protocol_for_terminal(&terminal_info_for_test(
                TerminalName::Kitty,
                Some(Multiplexer::Zellij { version: None }),
                Some("kitty"),
                /*term*/ None,
            )),
            None
        );
    }

    #[test]
    fn image_protocol_detects_iterm2_kitty_file_graphics() {
        for info in [
            terminal_info_with_version_for_test(
                TerminalName::Iterm2,
                /*multiplexer*/ None,
                Some("iTerm.app"),
                Some("3.6.10"),
                /*term*/ None,
            ),
            terminal_info_with_version_for_test(
                TerminalName::Unknown,
                /*multiplexer*/ None,
                Some("iTerm.app"),
                Some("3.6.10"),
                Some("xterm-256color"),
            ),
        ] {
            assert_eq!(
                image_protocol_for_terminal(&info),
                Some(ImageProtocol::KittyLocalFile)
            );
        }
    }

    #[test]
    fn image_protocol_rejects_old_iterm2_versions() {
        for info in [
            terminal_info_with_version_for_test(
                TerminalName::Iterm2,
                /*multiplexer*/ None,
                Some("iTerm.app"),
                Some("3.5.14"),
                /*term*/ None,
            ),
            terminal_info_with_version_for_test(
                TerminalName::Unknown,
                /*multiplexer*/ None,
                Some("iTerm.app"),
                /*version*/ None,
                Some("xterm-256color"),
            ),
            terminal_info_with_version_for_test(
                TerminalName::Iterm2,
                /*multiplexer*/ None,
                Some("iTerm.app"),
                Some("3.5"),
                /*term*/ None,
            ),
        ] {
            assert_eq!(
                image_protocol_for_terminal(&info),
                None
            );
        }
    }

    #[test]
    fn image_protocol_detects_kitty_graphics_terminals() {
        for info in [
            terminal_info_for_test(
                TerminalName::Ghostty,
                /*multiplexer*/ None,
                Some("Ghostty"),
                /*term*/ None,
            ),
            terminal_info_for_test(
                TerminalName::Kitty,
                /*multiplexer*/ None,
                Some("kitty"),
                /*term*/ None,
            ),
            terminal_info_for_test(
                TerminalName::WezTerm,
                /*multiplexer*/ None,
                Some("WezTerm"),
                /*term*/ None,
            ),
            terminal_info_for_test(
                TerminalName::Unknown,
                /*multiplexer*/ None,
                /*term_program*/ None,
                Some("xterm-kitty"),
            ),
            terminal_info_for_test(
                TerminalName::Unknown,
                /*multiplexer*/ None,
                /*term_program*/ None,
                Some("wezterm"),
            ),
            terminal_info_for_test(
                TerminalName::Unknown,
                /*multiplexer*/ None,
                Some("WezTerm"),
                Some("xterm-256color"),
            ),
        ] {
            assert_eq!(
                image_protocol_for_terminal(&info),
                Some(ImageProtocol::Kitty)
            );
        }
    }

    #[test]
    fn image_protocol_detects_sixel_terminals() {
        for info in [
            terminal_info_for_test(
                TerminalName::Unknown,
                /*multiplexer*/ None,
                /*term_program*/ None,
                Some("xterm-sixel"),
            ),
            terminal_info_for_test(
                TerminalName::Unknown,
                /*multiplexer*/ None,
                /*term_program*/ None,
                Some("foot"),
            ),
            terminal_info_for_test(
                TerminalName::Unknown,
                /*multiplexer*/ None,
                /*term_program*/ None,
                Some("mlterm"),
            ),
            terminal_info_for_test(
                TerminalName::WindowsTerminal,
                /*multiplexer*/ None,
                Some("WindowsTerminal"),
                Some("xterm-256color"),
            ),
        ] {
            assert_eq!(
                image_protocol_for_terminal(&info),
                Some(ImageProtocol::Sixel)
            );
        }
    }

    #[test]
    #[serial]
    fn wezterm_env_uses_kitty_graphics() {
        let _tmux = EnvVarGuard::new("TMUX", /*value*/ None);
        let _tmux_pane = EnvVarGuard::new("TMUX_PANE", /*value*/ None);
        let _zellij = EnvVarGuard::new("ZELLIJ", /*value*/ None);
        let _zellij_session = EnvVarGuard::new("ZELLIJ_SESSION_NAME", /*value*/ None);
        let _zellij_version = EnvVarGuard::new("ZELLIJ_VERSION", /*value*/ None);
        let _kitty = EnvVarGuard::new("KITTY_WINDOW_ID", /*value*/ None);
        let _wezterm = EnvVarGuard::new("WEZTERM_VERSION", Some("20240203"));
        let _wezterm_executable = EnvVarGuard::new("WEZTERM_EXECUTABLE", /*value*/ None);

        assert_eq!(
            detect_image_protocol(),
            Some(ImageProtocol::Kitty)
        );
    }

    #[test]
    fn image_protocol_rejects_unknown_terminals() {
        assert_eq!(
            image_protocol_for_terminal(&terminal_info_for_test(
                TerminalName::Unknown,
                /*multiplexer*/ None,
                /*term_program*/ None,
                Some("xterm-256color"),
            )),
            None
        );
    }

    fn terminal_info_for_test(
        name: TerminalName,
        multiplexer: Option<Multiplexer>,
        term_program: Option<&str>,
        term: Option<&str>,
    ) -> TerminalInfo {
        terminal_info_with_version_for_test(
            name,
            multiplexer,
            term_program,
            /*version*/ None,
            term,
        )
    }

    fn terminal_info_with_version_for_test(
        name: TerminalName,
        multiplexer: Option<Multiplexer>,
        term_program: Option<&str>,
        version: Option<&str>,
        term: Option<&str>,
    ) -> TerminalInfo {
        TerminalInfo {
            name,
            term_program: term_program.map(str::to_string),
            version: version.map(str::to_string),
            term: term.map(str::to_string),
            multiplexer,
        }
    }

    #[test]
    fn parse_dotted_version_requires_simple_numeric_components() {
        assert_eq!(parse_dotted_version(Some("3.6.10")), Some((3, 6, 10)));
        assert_eq!(parse_dotted_version(Some("3.6")), Some((3, 6, 0)));
        assert_eq!(parse_dotted_version(Some("3")), Some((3, 0, 0)));
        assert_eq!(parse_dotted_version(Some("3.6.10.1")), None);
        assert_eq!(parse_dotted_version(Some("3.6beta")), None);
        assert_eq!(parse_dotted_version(/*version*/ None), None);
    }

    #[test]
    #[serial]
    fn kitty_file_png_transmission_encodes_local_file_reference() {
        let _guard = EnvVarGuard::new("TMUX", /*value*/ None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.png");
        fs::write(&path, b"png").unwrap();

        let command = kitty_transmit_png_file_with_id(
            &path,
            /*columns*/ 4,
            /*rows*/ 3,
            /*image_id*/ Some(7),
        )
        .unwrap();
        let path = path.canonicalize().unwrap();
        let payload = general_purpose::STANDARD.encode(path.to_string_lossy().as_bytes());

        assert_eq!(
            command,
            format!("\x1b_Ga=T,t=f,f=100,c=4,r=3,q=2,i=7,C=1;{payload}\x1b\\")
        );
    }
}
