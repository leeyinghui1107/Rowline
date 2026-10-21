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
            256 => "New game",
            257 => "High scores",
            258 => "Exit",
            512 => "Next set",
            513 => "Show statistics",
            514 => "Use figures",
            515 => "Fixed scale",
            768 => "Contents",
            769 => "About",
            16528 => "Help",
            _ => "Unknown",
        }
    }

    /// The accelerator shown in the command reference.
    pub fn shortcut(&self) -> &'static str {
        match self.0 {
            256 => "F4",
            257 => "H",
            258 => "Alt+F9",
            512 => "F3",
            513 => "F5",
            514 => "F6",
            515 => "Ctrl+R",
            768 => "F1",
            769 => "",
            16528 => "",
            _ => "",
        }
    }

    /// New game -- `新建(&N)`, accelerator F4.
    pub const NEW_GAME: Command = Command(256);
    /// Show the high-score table -- `显示高分榜(&H)`.
    pub const SHOW_HIGH_SCORES: Command = Command(257);
    /// Quit -- `退出(&X)`, accelerator Alt+F9.
    pub const EXIT: Command = Command(258);
    /// Toggle the "next set" preview -- `下一组(&N)`, accelerator F3.
    pub const TOGGLE_NEXT_SET: Command = Command(512);
    /// Toggle the statistics readout -- `显示统计(&S)`, accelerator F5.
    pub const TOGGLE_STATISTICS: Command = Command(513);
    /// Toggle the polygon sprite set -- `使用形状`, accelerator F6.
    pub const TOGGLE_USE_FIGURES: Command = Command(514);
    /// Toggle the fixed-resolution display policy.
    ///
    /// This is a re-implementation extension (the original only has the
    /// `Ctrl+R` key); it uses an id outside the original's nine commands.
    pub const TOGGLE_FIXED_SCALE: Command = Command(515);
    /// Open the help contents -- `内容(&C)`, accelerator F1.
    pub const CONTENTS: Command = Command(768);
    /// Open the about box -- `关于(&A)...`.
    pub const ABOUT: Command = Command(769);
    /// Help -- `帮助(&H)`.
    pub const HELP: Command = Command(16528);

    /// The command a key press maps to, if any.
    pub fn from_key(key: rowline_core::input::Key) -> Option<Command> {
        use rowline_core::input::Key;
        Some(match key {
            Key::F1 => Command::CONTENTS,
            Key::F3 => Command::TOGGLE_NEXT_SET,
            Key::F4 => Command::NEW_GAME,
            Key::F5 => Command::TOGGLE_STATISTICS,
            Key::F6 => Command::TOGGLE_USE_FIGURES,
            _ => return None,
        })
    }
}

/// The icon strip's index of an icon.
///
/// The order matches `tools/make_icons.py`:
/// new, trophy, exit, next, stats, figures, help, contents, about, settings,
/// theme, crown, sword.
pub const ICON_NEW: u32 = 0;
pub const ICON_TROPHY: u32 = 1;
pub const ICON_EXIT: u32 = 2;
pub const ICON_NEXT: u32 = 3;
pub const ICON_STATS: u32 = 4;
pub const ICON_FIGURES: u32 = 5;
pub const ICON_HELP: u32 = 6;
pub const ICON_CONTENTS: u32 = 7;
pub const ICON_ABOUT: u32 = 8;
pub const ICON_SETTINGS: u32 = 9;
pub const ICON_THEME: u32 = 10;
/// The king's crown -- the mark of the CPU.
pub const ICON_CROWN: u32 = 11;
/// The challenger's sword -- the mark of the human.
pub const ICON_SWORD: u32 = 12;
/// The number of icons in the atlas.
pub const ICON_COUNT: u32 = 13;

/// The icon of a command, if it has one.
pub fn icon_for(cmd: Command) -> Option<u32> {
    Some(match cmd {
        Command::NEW_GAME => ICON_NEW,
        Command::SHOW_HIGH_SCORES => ICON_TROPHY,
        Command::EXIT => ICON_EXIT,
        Command::TOGGLE_NEXT_SET => ICON_NEXT,
        Command::TOGGLE_STATISTICS => ICON_STATS,
        Command::TOGGLE_USE_FIGURES => ICON_FIGURES,
        Command::TOGGLE_FIXED_SCALE => ICON_SETTINGS,
        Command::HELP => ICON_HELP,
        Command::CONTENTS => ICON_CONTENTS,
        Command::ABOUT => ICON_ABOUT,
        _ => return None,
    })
}

/// Every command, for building a menu.
pub const ALL: [Command; 10] = [
    Command::NEW_GAME,
    Command::SHOW_HIGH_SCORES,
    Command::EXIT,
    Command::TOGGLE_NEXT_SET,
    Command::TOGGLE_STATISTICS,
    Command::TOGGLE_USE_FIGURES,
    Command::TOGGLE_FIXED_SCALE,
    Command::HELP,
    Command::CONTENTS,
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
            .flat_map(|m| m.items.iter().filter_map(|i| i.id))
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
    fn every_command_has_an_icon() {
        for cmd in ALL {
            assert!(icon_for(cmd).is_some(), "command {} has no icon", cmd.0);
        }
    }

    #[test]
    fn accelerators_match_the_original() {
        assert_eq!(Command::from_key(Key::F3), Some(Command::TOGGLE_NEXT_SET));
        assert_eq!(Command::from_key(Key::F4), Some(Command::NEW_GAME));
        assert_eq!(Command::from_key(Key::F5), Some(Command::TOGGLE_STATISTICS));
        assert_eq!(
            Command::from_key(Key::F6),
            Some(Command::TOGGLE_USE_FIGURES)
        );
        assert_eq!(Command::from_key(Key::F1), Some(Command::CONTENTS));
        assert_eq!(Command::from_key(Key::KeyA), None);
    }
}
