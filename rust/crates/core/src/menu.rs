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
    /// The command id.
    pub id: u16,
    /// The label.
    pub label: &'static str,
    /// The accelerator hint shown on the right.
    pub hint: &'static str,
}

impl MenuItem {
    /// A command item.
    pub const fn item(id: u16, label: &'static str, hint: &'static str) -> MenuItem {
        MenuItem { id, label, hint }
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
            // A compact menu: 26 px bar and rows, matching the density of a
            // desktop menu bar.
            bar_height: 26.0,
            row_height: 26.0,
            row_width: 200.0,
            title_width: 88.0,
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

    /// The first row of a dropdown.
    pub fn first_selectable(&self, menu: usize) -> Option<usize> {
        (!self.menus.get(menu)?.items.is_empty()).then_some(0)
    }

    /// The row under the pointer, given the pointer's position relative to the
    /// dropdown panel.
    pub fn hover_at(&mut self, panel_y: f32) {
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
        self.hover = Some(idx);
    }

    /// Move the hover by `delta` rows, wrapping around.
    pub fn move_hover(&mut self, delta: i32) {
        let Some(menu) = self.open else { return };
        let Some(m) = self.menus.get(menu) else {
            return;
        };
        if m.items.is_empty() {
            return;
        }
        let n = m.items.len() as i32;
        let idx = (self.hover.unwrap_or(0) as i32 + delta).rem_euclid(n);
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
        Some(self.menus.get(menu)?.items.get(row)?.id)
    }

    /// Activate the hovered command and close the menu.
    pub fn activate(&mut self) -> Option<u16> {
        let cmd = self.hovered_command();
        self.close();
        cmd
    }

    /// The command under the pointer, activating it.
    pub fn click(&mut self, panel_y: f32) -> Option<u16> {
        self.hover_at(panel_y);
        self.activate()
    }
}

/// The menu tree of the final redesign.
///
/// Two compact menus, no separator rows: the game commands and the
/// statistics toggle live in `文件`, and the help entries in `帮助`.
pub fn default_menus() -> Vec<Menu> {
    vec![
        Menu {
            title: "文件(F)",
            items: vec![
                MenuItem::item(256, "新游戏", "F4"),
                MenuItem::item(257, "高分榜", "H"),
                MenuItem::item(513, "显示统计", "F5"),
                MenuItem::item(258, "退出", "Alt+F9"),
            ],
        },
        Menu {
            title: "帮助(H)",
            items: vec![
                MenuItem::item(16528, "帮助主题", "F1"),
                MenuItem::item(769, "关于 Rowline", ""),
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
    fn the_menu_ids_match_the_redesign() {
        let m = model();
        let ids: Vec<u16> = m
            .menus
            .iter()
            .flat_map(|x| x.items.iter().map(|i| i.id))
            .collect();
        for expected in [256u16, 257, 513, 258, 16528, 769] {
            assert!(ids.contains(&expected), "missing command {expected}");
        }
        for removed in [512u16, 514, 515, 516, 517, 518, 768] {
            assert!(!ids.contains(&removed), "command {removed} should be gone");
        }
        assert_eq!(m.menus.len(), 2);
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
        // The first row of "File" is "New".
        assert_eq!(m.hover, Some(0));
        assert_eq!(m.hovered_command(), Some(256));
    }

    #[test]
    fn the_hover_moves_through_every_item() {
        let mut m = model();
        m.toggle(0);
        m.move_hover(1);
        assert_eq!(m.hovered_command(), Some(257));
        m.move_hover(1);
        assert_eq!(m.hovered_command(), Some(513));
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
        // Row 2 is "显示统计" (id 513).
        let cmd = m.click(row_h * 2.0 + 1.0);
        assert_eq!(cmd, Some(513));
        assert!(m.is_closed());
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
