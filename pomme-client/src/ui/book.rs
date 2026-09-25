use crate::renderer::pipelines::menu_overlay::MenuElement;
use crate::ui::common;
use crate::ui::text_edit::{MultilineField, SystemClipboard, TextFieldState, TextInputEvent};

/// State for a server-opened writable book. The server remains authoritative;
/// edits are submitted through `ServerboundEditBook` when the player saves or
/// signs.
pub struct BookEditState {
    pub slot: u32,
    pub pages: Vec<String>,
    pub page: usize,
    pub title: TextFieldState,
    pub author: String,
    pub title_focused: bool,
    field: MultilineField,
}

impl BookEditState {
    pub fn new(slot: u32, pages: Vec<String>, title: String, author: String) -> Self {
        let wf = |s: &str| s.chars().count() as f32 * 6.0;
        let mut field = MultilineField::new(1024, None);
        field.set_width(250.0, &wf);
        field.set_value(pages.first().map_or("", String::as_str), &wf);
        let mut title_field = TextFieldState::new(32);
        title_field.set_value(&title, 250.0, &|s| s.chars().count() as f32 * 6.0);
        Self {
            slot,
            pages: if pages.is_empty() {
                vec![String::new()]
            } else {
                pages
            },
            page: 0,
            title: title_field,
            author,
            title_focused: false,
            field,
        }
    }

    fn store_page(&mut self) {
        self.pages[self.page] = self.field.value().to_owned();
    }

    pub fn navigate(&mut self, action: usize) {
        match action {
            0 => self.switch_page(self.page.saturating_sub(1)),
            1 => self.switch_page(self.page + 1),
            _ => {}
        }
    }

    fn switch_page(&mut self, page: usize) {
        self.store_page();
        self.page = page.min(99);
        if self.page == self.pages.len() {
            self.pages.push(String::new());
        }
        let wf = |s: &str| s.chars().count() as f32 * 6.0;
        self.field.set_value(&self.pages[self.page], &wf);
    }

    /// True when the UI should close after an explicit save/sign action.
    pub fn input(
        &mut self,
        events: &[TextInputEvent],
        save: bool,
        sign: bool,
    ) -> Option<(u32, Vec<String>, Option<String>)> {
        let mut clipboard = SystemClipboard;
        for event in events {
            match event {
                TextInputEvent::Key { code, .. } if *code == winit::keyboard::KeyCode::Tab => {
                    self.title_focused = !self.title_focused;
                }
                TextInputEvent::Key { code, .. } if *code == winit::keyboard::KeyCode::PageDown => {
                    self.switch_page(self.page + 1)
                }
                TextInputEvent::Key { code, .. } if *code == winit::keyboard::KeyCode::PageUp => {
                    self.switch_page(self.page.saturating_sub(1))
                }
                TextInputEvent::Key { code, mods }
                    if matches!(
                        code,
                        winit::keyboard::KeyCode::Enter | winit::keyboard::KeyCode::NumpadEnter
                    ) && mods.edit_shortcut() => {}
                TextInputEvent::Key { code, mods } => {
                    let wf = |s: &str| s.chars().count() as f32 * 6.0;
                    if self.title_focused {
                        self.title
                            .key_pressed(*code, mods, &mut clipboard, 250.0, &wf);
                    } else {
                        self.field.handle(event, &mut clipboard, &wf);
                        self.store_page();
                    }
                }
                TextInputEvent::Char(ch) => {
                    let wf = |s: &str| s.chars().count() as f32 * 6.0;
                    if self.title_focused {
                        self.title.char_typed(*ch, 250.0, &wf);
                    } else {
                        self.field.handle(event, &mut clipboard, &wf);
                        self.store_page();
                    }
                }
            }
        }
        if save || (sign && !self.title.value().trim().is_empty()) {
            self.store_page();
            Some((
                self.slot,
                self.pages.clone(),
                sign.then(|| self.title.value().to_owned()),
            ))
        } else {
            None
        }
    }

    pub fn draw(
        &self,
        elements: &mut Vec<MenuElement>,
        sw: f32,
        sh: f32,
        gs: f32,
        cursor: (f32, f32),
    ) {
        let w = 300.0 * gs;
        let h = 220.0 * gs;
        let x = (sw - w) / 2.0;
        let y = (sh - h) / 2.0;
        elements.push(MenuElement::Rect {
            x,
            y,
            w,
            h,
            corner_radius: 4.0,
            color: [0.90, 0.84, 0.68, 1.0],
        });
        let scale = common::FONT_SIZE * gs;
        elements.push(MenuElement::Text {
            x: x + 12.0 * gs,
            y: y + 10.0 * gs,
            text: format!(
                "Book  {}/{}",
                self.page + 1,
                self.pages.len().max(self.page + 1)
            ),
            scale,
            color: [0.12, 0.10, 0.08, 1.0],
            centered: false,
        });
        elements.push(MenuElement::Text {
            x: x + 12.0 * gs,
            y: y + 30.0 * gs,
            text: format!("Author: {}", self.author),
            scale,
            color: [0.12, 0.10, 0.08, 1.0],
            centered: false,
        });
        elements.push(MenuElement::Text {
            x: x + 12.0 * gs,
            y: y + 54.0 * gs,
            text: format!(
                "Title: {}{}",
                self.title.value(),
                if self.title_focused { "_" } else { "" }
            ),
            scale,
            color: [0.12, 0.10, 0.08, 1.0],
            centered: false,
        });
        let field_x = x + 12.0 * gs;
        let field_y = y + 78.0 * gs;
        elements.push(MenuElement::Rect {
            x: field_x,
            y: field_y,
            w: w - 24.0 * gs,
            h: 95.0 * gs,
            corner_radius: 0.0,
            color: [0.97, 0.94, 0.84, 1.0],
        });
        let line_height = scale + 2.0 * gs;
        for (index, (start, end)) in self.field.lines().iter().copied().enumerate() {
            if field_y + 4.0 * gs + index as f32 * line_height >= field_y + 91.0 * gs {
                break;
            }
            elements.push(MenuElement::Text {
                x: field_x + 4.0 * gs,
                y: field_y + 4.0 * gs + index as f32 * line_height,
                text: self.field.value()[start..end].to_owned(),
                scale,
                color: [0.12, 0.10, 0.08, 1.0],
                centered: false,
            });
        }
        let controls = [
            ("Prev", x + 12.0 * gs),
            ("Next", x + 74.0 * gs),
            ("Save", x + w - 112.0 * gs),
            ("Sign", x + w - 54.0 * gs),
        ];
        for (label, bx) in controls {
            let rect = [bx, y + h - 30.0 * gs, 48.0 * gs, 20.0 * gs];
            let hovered = common::hit_test(cursor, rect);
            elements.push(MenuElement::Rect {
                x: rect[0],
                y: rect[1],
                w: rect[2],
                h: rect[3],
                corner_radius: 2.0,
                color: if hovered {
                    [0.65, 0.56, 0.40, 1.0]
                } else {
                    [0.76, 0.68, 0.52, 1.0]
                },
            });
            elements.push(MenuElement::Text {
                x: bx + 24.0 * gs,
                y: y + h - 26.0 * gs,
                text: label.into(),
                scale,
                color: [0.12, 0.10, 0.08, 1.0],
                centered: true,
            });
        }
    }
}

/// Read-only book opened by the server's OpenBook packet.
pub struct BookViewState {
    pub pages: Vec<String>,
    pub page: usize,
    field: MultilineField,
}

impl BookViewState {
    pub fn new(pages: Vec<String>) -> Self {
        let pages = if pages.is_empty() {
            vec![String::new()]
        } else {
            pages
        };
        let wf = |s: &str| s.chars().count() as f32 * 6.0;
        let mut field = MultilineField::new(1024, None);
        field.set_width(250.0, &wf);
        field.set_value(&pages[0], &wf);
        Self {
            pages,
            page: 0,
            field,
        }
    }

    pub fn navigate(&mut self, action: usize) {
        let page = match action {
            0 => self.page.saturating_sub(1),
            1 => (self.page + 1).min(self.pages.len() - 1),
            _ => return,
        };
        self.page = page;
        let wf = |s: &str| s.chars().count() as f32 * 6.0;
        self.field.set_value(&self.pages[page], &wf);
    }

    pub fn draw(&self, elements: &mut Vec<MenuElement>, sw: f32, sh: f32, gs: f32) {
        let w = 300.0 * gs;
        let h = 220.0 * gs;
        let x = (sw - w) / 2.0;
        let y = (sh - h) / 2.0;
        elements.push(MenuElement::Rect {
            x,
            y,
            w,
            h,
            corner_radius: 4.0,
            color: [0.90, 0.84, 0.68, 1.0],
        });
        let scale = common::FONT_SIZE * gs;
        elements.push(MenuElement::Text {
            x: x + w / 2.0,
            y: y + 10.0 * gs,
            text: format!("{}/{}", self.page + 1, self.pages.len()),
            scale,
            color: [0.12, 0.10, 0.08, 1.0],
            centered: true,
        });
        for (line, (start, end)) in self.field.lines().iter().copied().take(9).enumerate() {
            elements.push(MenuElement::Text {
                x: x + 12.0 * gs,
                y: y + (34.0 + line as f32 * 16.0) * gs,
                text: self.pages[self.page][start..end].to_owned(),
                scale,
                color: [0.12, 0.10, 0.08, 1.0],
                centered: false,
            });
        }
        for (label, bx) in [("Prev", x + 12.0 * gs), ("Next", x + 74.0 * gs)] {
            elements.push(MenuElement::Text {
                x: bx + 24.0 * gs,
                y: y + h - 26.0 * gs,
                text: label.into(),
                scale,
                color: [0.12, 0.10, 0.08, 1.0],
                centered: true,
            });
        }
    }
}

pub fn clicked_action(cursor: (f32, f32), sw: f32, sh: f32, gs: f32) -> Option<usize> {
    let w = 300.0 * gs;
    let h = 220.0 * gs;
    let x = (sw - w) / 2.0;
    let y = (sh - h) / 2.0;
    [
        x + 12.0 * gs,
        x + 74.0 * gs,
        x + w - 112.0 * gs,
        x + w - 54.0 * gs,
    ]
    .iter()
    .position(|bx| common::hit_test(cursor, [*bx, y + h - 30.0 * gs, 48.0 * gs, 20.0 * gs]))
}

#[cfg(test)]
mod view_tests {
    use super::BookViewState;

    #[test]
    fn book_view_page_navigation_stays_within_pages() {
        let mut book = BookViewState::new(vec!["one".into(), "two".into()]);
        book.navigate(0);
        assert_eq!(book.page, 0);
        book.navigate(1);
        book.navigate(1);
        assert_eq!(book.page, 1);
        book.navigate(0);
        assert_eq!(book.page, 0);
    }
}
