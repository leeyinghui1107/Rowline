//! The menu model.
//!
//! The original used a Win32 menu bar.  The re-implementation draws its own
//! menu in the canvas, so that it looks the same on every platform and can
//! use the modern design system -- but the **command ids are the original's**,
//! so the behaviour can be compared against the original (docs/03 S12.1).
//!
//! The model is pure logic: it knows the tree, what is open, what is
//! hovered, and how the keyboard moves around.  It knows nothing about
//! pixels -- the geometry lives in [`MenuMetrics`], so that the model and the
//! renderer agree on where things are.

/// One entry in a dropdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// The command id, or `None` for a separator.
    pub id: Option<u16>,
    /// The label.
    pub label: &'static str,
    /// The accelerator hint shown on the right.
    pub hint: &'static str,
}

impl MenuItem {
    /// A command item.
    pub const fn item(id: u16, label: &'static str, hint: &'static str) -> MenuItem {
        MenuItem {
            id: Some(id),
            label,
            hint,
        }
    }

    /// A separator.
    pub const fn separator() -> MenuItem {
        MenuItem {
            id: None,
            label: "",
            hint: "",
        }
    }

    /// `true` when this is a separator.
    pub const fn is_separator(&self) -> bool {
        self.id.is_none()
    }
}

/// A dropdown menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    /// The title shown in the menu bar.
    pub title: &'static str,
    /// The items, in order.
    pub items: Vec<MenuItem>,
}

/// The metrics of the drawn menu, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuMetrics {
    /// The height of the menu bar.
    pub bar_height: f32,
    /// The height of one dropdown row.
    pub row_height: f32,
    /// The width of the widest label, plus the hint column.
    pub row_width: f32,
    /// The width of a title in the menu bar.
    pub title_width: f32,
    /// The inner padding.
    pub pad: f32,
}

impl Default for MenuMetrics {
    fn default() -> Self {
        MenuMetrics {
            // The original uses a native Win32 menu: ~20 logical pixels tall,
            // with 12 px labels.
            bar_height: 20.0,
            row_height: 20.0,
            row_width: 160.0,
            title_width: 56.0,
            pad: 6.0,
        }
    }
}

/// The menu state.
#[derive(Clone, Debug, Default)]
pub struct MenuModel {
    /// The dropdowns, in bar order.
    pub menus: Vec<Menu>,
    /// The open dropdown, if any.
    pub open: Option<usize>,
    /// The hovered row inside the open dropdown.
    pub hover: Option<usize>,
    /// The metrics used for the drawing.
    pub metrics: MenuMetrics,
}

impl MenuModel {
    /// A model over a set of dropdowns.
    pub fn new(menus: Vec<Menu>) -> Self {
        MenuModel {
            menus,
            open: None,
            hover: None,
            metrics: MenuMetrics::default(),
        }
    }

    /// `true` when nothing is open.
    pub fn is_closed(&self) -> bool {
        self.open.is_none()
    }

    /// Open a dropdown, or close it when it is already open.
    pub fn toggle(&mut self, index: usize) {
        if self.open == Some(index) {
            self.close();
        } else {
            self.open = Some(index);
            self.hover = self.first_selectable(index);
        }
    }

    /// Close the dropdown.
    pub fn close(&mut self) {
        self.open = None;
        self.hover = None;
    }

    /// Open the menu bar and focus the first dropdown.
    pub fn open_bar(&mut self) {
        if self.open.is_none() {
            self.open = Some(0);
            self.hover = self.first_selectable(0);
        }
    }

    /// The first selectable row of a dropdown.
    pub fn first_selectable(&self, menu: usize) -> Option<usize> {
        self.menus
            .get(menu)?
            .items
            .iter()
            .position(|i| !i.is_separator())
    }

    /// The row under the pointer, given the pointer's position relative to the
    /// dropdown panel.
    pub fn hover_at(&mut self, panel_x: f32, panel_y: f32) {
        let Some(menu) = self.open else { return };
        let Some(m) = self.menus.get(menu) else {
            return;
        };
        let row = (panel_y / self.metrics.row_height).floor() as isize;
        if row < 0 || row as usize >= m.items.len() {
            self.hover = None;
            return;
        }
        let idx = row as usize;
        self.hover = if m.items[idx].is_separator() {
            None
        } else {
            Some(idx)
        };
        let _ = panel_x;
    }

    /// Move the hover by `delta` rows, skipping separators.
    pub fn move_hover(&mut self, delta: i32) {
        let Some(menu) = self.open else { return };
        let Some(m) = self.menus.get(menu) else {
            return;
        };
        if m.items.is_empty() {
            return;
        }
        let n = m.items.len() as i32;
        let mut idx = self.hover.unwrap_or(0) as i32;
        for _ in 0..n {
            idx = (idx + delta).rem_euclid(n);
            if !m.items[idx as usize].is_separator() {
                break;
            }
        }
        self.hover = Some(idx as usize);
    }

    /// Move to the next dropdown, wrapping around.
    pub fn next_menu(&mut self, delta: i32) {
        if self.menus.is_empty() {
            return;
        }
        let n = self.menus.len() as i32;
        let cur = self.open.unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(n);
        self.open = Some(next as usize);
        self.hover = self.first_selectable(next as usize);
    }

    /// The hovered command, if any.
    pub fn hovered_command(&self) -> Option<u16> {
        let menu = self.open?;
        let row = self.hover?;
        self.menus.get(menu)?.items.get(row)?.id
    }

    /// Activate the hovered command and close the menu.
    pub fn activate(&mut self) -> Option<u16> {
        let cmd = self.hovered_command();
        self.close();
        cmd
    }

    /// The command under the pointer, activating it.
    pub fn click(&mut self, panel_x: f32, panel_y: f32) -> Option<u16> {
        self.hover_at(panel_x, panel_y);
        self.activate()
    }
}

/// The default menu tree, mirroring the original's command ids.
pub fn default_menus() -> Vec<Menu> {
    vec![
        Menu {
            // The strings are the original's, as parsed from its RT_MENU
            // resource (analysis/menu_tree.txt): the interface is
            // Simplified Chinese.
            // The accelerator is part of the original title.
            title: "\u{6587}\u{4ef6}(F)",
            items: vec![
                MenuItem::item(256, "\u{65b0}\u{5efa}", "F4"),
                MenuItem::separator(),
                MenuItem::item(257, "\u{663e}\u{793a}\u{9ad8}\u{5206}\u{699c}", ""),
                MenuItem::separator(),
                MenuItem::item(258, "\u{9000}\u{51fa}", "Alt+F9"),
            ],
        },
        Menu {
            title: "\u{9009}\u{9879}(O)",
            items: vec![
                MenuItem::item(512, "\u{4e0b}\u{4e00}\u{7ec4}", "F3"),
                MenuItem::item(513, "\u{663e}\u{793a}\u{7edf}\u{8ba1}", "F5"),
                MenuItem::item(514, "\u{4f7f}\u{7528}\u{5f62}\u{72b6}", "F6"),
                // Re-implementation extension: the original only exposes
                // this through the `Ctrl+R` display policy, never as a menu
                // command.  Id 515 is outside the original's nine ids.
                MenuItem::item(515, "\u{56fa}\u{5b9a}\u{5206}\u{8fa8}\u{7387}", "Ctrl+R"),
                MenuItem::separator(),
                MenuItem::item(16528, "\u{5e2e}\u{52a9}", ""),
                MenuItem::item(768, "\u{5185}\u{5bb9}", "F1"),
                MenuItem::item(769, "\u{5173}\u{4e8e}", ""),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> MenuModel {
        MenuModel::new(default_menus())
    }

    #[test]
    fn the_command_ids_match_the_original() {
        let m = model();
        let ids: Vec<u16> = m
            .menus
            .iter()
            .flat_map(|x| x.items.iter().filter_map(|i| i.id))
            .collect();
        for expected in [256u16, 257, 258, 512, 513, 514, 515, 16528, 768, 769] {
            assert!(ids.contains(&expected), "missing command {expected}");
        }
    }

    #[test]
    fn toggling_opens_and_closes() {
        let mut m = model();
        assert!(m.is_closed());
        m.toggle(0);
        assert_eq!(m.open, Some(0));
        m.toggle(0);
        assert!(m.is_closed());
    }

    #[test]
    fn opening_focuses_a_real_item() {
        let mut m = model();
        m.toggle(0);
        // The first row of "File" is "New", not a separator.
        assert_eq!(m.hover, Some(0));
        assert_eq!(m.hovered_command(), Some(256));
    }

    #[test]
    fn the_hover_skips_separators() {
        let mut m = model();
        m.toggle(0);
        m.move_hover(1); // -> row 1 is a separator, so it lands on row 2
        assert_eq!(m.hovered_command(), Some(257));
        m.move_hover(1);
        assert_eq!(m.hovered_command(), Some(258));
    }

    #[test]
    fn the_hover_wraps_around() {
        let mut m = model();
        m.toggle(0);
        m.move_hover(-1);
        assert_eq!(m.hovered_command(), Some(258));
    }

    #[test]
    fn clicking_picks_the_row_under_the_pointer() {
        let mut m = model();
        m.toggle(0);
        let row_h = m.metrics.row_height;
        // Row 2 is "High Scores" (id 257).
        let cmd = m.click(10.0, row_h * 2.0 + 1.0);
        assert_eq!(cmd, Some(257));
        assert!(m.is_closed());
    }

    #[test]
    fn clicking_a_separator_activates_nothing() {
        let mut m = model();
        m.toggle(0);
        let row_h = m.metrics.row_height;
        assert_eq!(m.click(10.0, row_h * 1.0 + 1.0), None);
    }

    #[test]
    fn the_menu_bar_cycles_between_dropdowns() {
        let mut m = model();
        m.open_bar();
        assert_eq!(m.open, Some(0));
        m.next_menu(1);
        assert_eq!(m.open, Some(1));
        m.next_menu(1);
        assert_eq!(m.open, Some(0));
    }

    #[test]
    fn activating_returns_the_command_and_closes() {
        let mut m = model();
        m.toggle(0);
        assert_eq!(m.activate(), Some(256));
        assert!(m.is_closed());
    }
}
