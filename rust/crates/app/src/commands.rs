//! Menu command identifiers.
//!
//! These are the command IDs recovered from the original's `RT_MENU`
//! resource (see `docs/03_FEATURE_SPEC.md` section 12).  They are kept
//! verbatim so that the behaviour of the original can be reproduced and
//! diffed, and so that the menu and the accelerators stay in sync.

/// The command a menu item or accelerator carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command(pub u16);

impl Command {
    /// The label shown in the command reference.
    pub fn label(&self) -> &'static str {
        match self.0 {
            256 => "新游戏",
            257 => "高分榜",
            258 => "退出",
            513 => "显示统计",
            769 => "关于 Rowline",
            16528 => "帮助",
            _ => "未知命令",
        }
    }

    /// The accelerator shown in the command reference.
    ///
    /// macOS uses the command key for the presentation shortcuts; the
    /// labels follow the platform so the menu does not lie.
    pub fn shortcut(&self) -> &'static str {
        match self.0 {
            256 => "F4",
            257 => "H",
            258 => "Alt+F9",
            513 => "F5",
            769 => "",
            16528 => "F1",
            _ => "",
        }
    }

    /// New game -- `新建`, accelerator F4.
    pub const NEW_GAME: Command = Command(256);
    /// Show the high-score table -- `高分榜`, accelerator H.
    pub const SHOW_HIGH_SCORES: Command = Command(257);
    /// Quit -- `退出`, accelerator Alt+F9.
    pub const EXIT: Command = Command(258);
    /// Open the statistics dialog -- `显示统计`, accelerator F5.
    pub const TOGGLE_STATISTICS: Command = Command(513);
    /// Open the about box -- `关于 Rowline`.
    pub const ABOUT: Command = Command(769);
    /// Open the help dialog -- `帮助`, accelerator F1.
    pub const HELP: Command = Command(16528);

    /// The command a key press maps to, if any.
    pub fn from_key(key: rowline_core::input::Key) -> Option<Command> {
        use rowline_core::input::Key;
        Some(match key {
            Key::F1 => Command::HELP,
            Key::F4 => Command::NEW_GAME,
            Key::F5 => Command::TOGGLE_STATISTICS,
            // Win32 menu mnemonic for 高分榜.
            Key::KeyH => Command::SHOW_HIGH_SCORES,
            _ => return None,
        })
    }
}

/// The icon strip's only used slot: the challenger's sword.
///
/// The atlas still carries the original thirteen icons; the rest are not
/// referenced by the final two-menu design.
pub const ICON_SWORD: u32 = 12;
/// The number of cells in the icon atlas.
pub const ICON_COUNT: u32 = 13;

/// Every command, for the menu and the in-app reference.
pub const ALL: [Command; 6] = [
    Command::NEW_GAME,
    Command::SHOW_HIGH_SCORES,
    Command::TOGGLE_STATISTICS,
    Command::EXIT,
    Command::HELP,
    Command::ABOUT,
];

#[cfg(test)]
mod tests {
    use super::*;
    use rowline_core::input::Key;

    #[test]
    fn the_menu_covers_every_command() {
        // The menu model and the command list must not drift apart.
        let menus = rowline_core::menu::default_menus();
        let in_menu: Vec<u16> = menus
            .iter()
            .flat_map(|m| m.items.iter().map(|i| i.id))
            .collect();
        for cmd in ALL {
            assert!(
                in_menu.contains(&cmd.0),
                "command {} is missing from the menu",
                cmd.0
            );
        }
        assert_eq!(in_menu.len(), ALL.len(), "the menu has extra commands");
    }

    #[test]
    fn accelerators_match_the_redesign() {
        assert_eq!(Command::from_key(Key::F4), Some(Command::NEW_GAME));
        assert_eq!(Command::from_key(Key::F5), Some(Command::TOGGLE_STATISTICS));
        assert_eq!(Command::from_key(Key::F1), Some(Command::HELP));
        assert_eq!(
            Command::from_key(Key::KeyH),
            Some(Command::SHOW_HIGH_SCORES)
        );
        assert_eq!(Command::from_key(Key::F3), None);
        assert_eq!(Command::from_key(Key::F6), None);
        assert_eq!(Command::from_key(Key::KeyA), None);
    }
}
