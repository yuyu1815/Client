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
    pub pages: Vec<azalea_chat::FormattedText>,
    pub page: usize,
    hit_regions: Vec<crate::ui::chat::StyleHitRegion>,
}

impl BookViewState {
    pub fn new(pages: Vec<azalea_chat::FormattedText>) -> Self {
        let pages = if pages.is_empty() {
            vec![azalea_chat::FormattedText::default()]
        } else {
            pages
        };
        Self {
            pages,
            page: 0,
            hit_regions: Vec::new(),
        }
    }

    pub fn navigate(&mut self, action: usize) {
        let page = match action {
            0 => self.page.saturating_sub(1),
            1 => (self.page + 1).min(self.pages.len() - 1),
            _ => return,
        };
        self.set_page(page);
    }

    pub fn set_page(&mut self, page: usize) {
        let page = page.min(self.pages.len().saturating_sub(1));
        if self.page != page {
            self.page = page;
            self.hit_regions.clear();
        }
    }

    pub fn draw(
        &mut self,
        elements: &mut Vec<MenuElement>,
        sw: f32,
        sh: f32,
        gs: f32,
        cursor: (f32, f32),
        spans_width: &dyn Fn(&[crate::ui::text::TextSpan], f32) -> f32,
    ) {
        let layout = BookViewLayout::new(sw, sh, gs, self.page, self.pages.len());
        let x = layout.x;
        let y = layout.y;
        let w = layout.w;
        let h = layout.h;
        elements.push(MenuElement::CroppedImage {
            x,
            y,
            w,
            h,
            sprite: crate::renderer::pipelines::menu_overlay::SpriteId::BookBackground,
            uv_range: [0.0, 0.0, 0.75, 0.75],
            tint: [1.0; 4],
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
        let spans = crate::ui::text::format_book_text_spans(
            &self.pages[self.page],
            [0.12, 0.10, 0.08, 1.0],
        );
        let lines = crate::ui::chat::wrap_spans(&spans, 114.0, &|line| {
            spans_width(line, common::FONT_SIZE)
        });
        let origin_x = x + 36.0 * gs;
        let origin_y = y + 30.0 * gs;
        self.hit_regions.clear();
        for (index, line) in lines.iter().take(14).enumerate() {
            if index == 0 {
                elements.push(MenuElement::TextSpans {
                    x: origin_x,
                    y: origin_y,
                    spans: line.clone(),
                    scale,
                    centered: false,
                });
            } else {
                elements.push(MenuElement::TextSpans {
                    x: origin_x,
                    y: origin_y + index as f32 * 9.0 * gs,
                    spans: line.clone(),
                    scale,
                    centered: false,
                });
            }
            crate::ui::chat::push_hit_regions(
                &mut self.hit_regions,
                line,
                origin_x,
                origin_y + index as f32 * 9.0 * gs,
                9.0 * gs,
                &|span| spans_width(std::slice::from_ref(span), scale),
            );
        }
        for (rect, visible, normal, highlighted) in [
            (
                layout.previous,
                self.page > 0,
                crate::renderer::pipelines::menu_overlay::SpriteId::BookPageBackward,
                crate::renderer::pipelines::menu_overlay::SpriteId::BookPageBackwardHighlighted,
            ),
            (
                layout.next,
                self.page + 1 < self.pages.len(),
                crate::renderer::pipelines::menu_overlay::SpriteId::BookPageForward,
                crate::renderer::pipelines::menu_overlay::SpriteId::BookPageForwardHighlighted,
            ),
        ] {
            if visible {
                let sprite = if common::hit_test(cursor, rect) {
                    highlighted
                } else {
                    normal
                };
                elements.push(MenuElement::Image {
                    x: rect[0],
                    y: rect[1],
                    w: rect[2],
                    h: rect[3],
                    sprite,
                    tint: [1.0; 4],
                });
            }
        }
        let hovered = common::hit_test(cursor, layout.done);
        elements.push(MenuElement::NineSlice {
            x: layout.done[0],
            y: layout.done[1],
            w: layout.done[2],
            h: layout.done[3],
            sprite: if hovered {
                crate::renderer::pipelines::menu_overlay::SpriteId::ButtonHover
            } else {
                crate::renderer::pipelines::menu_overlay::SpriteId::ButtonNormal
            },
            border: 3.0 * gs,
            tint: [1.0; 4],
        });
        elements.push(MenuElement::Text {
            x: layout.done[0] + layout.done[2] / 2.0,
            y: layout.done[1] + (layout.done[3] - common::FONT_SIZE * gs) / 2.0 + gs,
            text: "Done".into(),
            scale,
            color: [1.0; 4],
            centered: true,
        });
    }

    pub fn style_at(
        &self,
        cursor: (f32, f32),
    ) -> Option<std::sync::Arc<crate::chat_component::ResolvedStyle>> {
        crate::ui::chat::style_at(&self.hit_regions, cursor)
    }
}

#[derive(Clone, Copy)]
struct BookViewLayout {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    previous: [f32; 4],
    next: [f32; 4],
    done: [f32; 4],
}

impl BookViewLayout {
    fn new(sw: f32, sh: f32, gs: f32, page: usize, pages: usize) -> Self {
        let w = 192.0 * gs;
        let h = 192.0 * gs;
        let x = (sw - w) / 2.0;
        let y = 2.0 * gs;
        Self {
            x,
            y,
            w,
            h,
            previous: [x + 43.0 * gs, y + 157.0 * gs, 23.0 * gs, 13.0 * gs],
            next: [x + 116.0 * gs, y + 157.0 * gs, 23.0 * gs, 13.0 * gs],
            done: [
                (sw - 50.0 * gs) / 2.0,
                y + h + 8.0 * gs,
                50.0 * gs,
                20.0 * gs,
            ],
        }
    }
}

pub fn view_hit_action(
    cursor: (f32, f32),
    sw: f32,
    sh: f32,
    gs: f32,
    page: usize,
    pages: usize,
) -> Option<usize> {
    let layout = BookViewLayout::new(sw, sh, gs, page, pages);
    if page > 0 && common::hit_test(cursor, layout.previous) {
        Some(0)
    } else if page + 1 < pages && common::hit_test(cursor, layout.next) {
        Some(1)
    } else {
        None
    }
}
pub fn view_done_hit(cursor: (f32, f32), sw: f32, sh: f32, gs: f32) -> bool {
    common::hit_test(cursor, BookViewLayout::new(sw, sh, gs, 0, 1).done)
}

pub fn edit_hit_action(cursor: (f32, f32), sw: f32, sh: f32, gs: f32) -> Option<usize> {
    let w = 300.0 * gs;
    let h = 220.0 * gs;
    let x = (sw - w) / 2.0;
    let y = (sh - h) / 2.0;
    let controls = [
        x + 12.0 * gs,
        x + 74.0 * gs,
        x + w - 112.0 * gs,
        x + w - 54.0 * gs,
    ];
    controls
        .iter()
        .position(|bx| common::hit_test(cursor, [*bx, y + h - 30.0 * gs, 48.0 * gs, 20.0 * gs]))
}

pub fn clicked_action(cursor: (f32, f32), sw: f32, sh: f32, gs: f32) -> Option<usize> {
    view_hit_action(cursor, sw, sh, gs, 0, 2)
}

#[cfg(test)]
mod view_tests {
    use winit::event::{ElementState, MouseButton};

    use super::{BookViewState, view_hit_action};
    use crate::app::input::InputState;

    #[test]
    fn official_book_layout_and_arrow_hit_centers() {
        let layout = super::BookViewLayout::new(1000.0, 700.0, 2.0, 0, 2);
        assert_eq!(
            [layout.x, layout.y, layout.w, layout.h],
            [308.0, 4.0, 384.0, 384.0]
        );
        assert_eq!(layout.next, [540.0, 318.0, 46.0, 26.0]);
        assert_eq!(layout.previous, [394.0, 318.0, 46.0, 26.0]);
        assert_eq!(
            view_hit_action((563.0, 331.0), 1000.0, 700.0, 2.0, 0, 2),
            Some(1)
        );
        assert_eq!(
            view_hit_action((417.0, 331.0), 1000.0, 700.0, 2.0, 1, 2),
            Some(0)
        );
        let mut book = BookViewState::new(
            vec!["one", "two", "three"]
                .into_iter()
                .map(azalea_chat::FormattedText::from)
                .collect(),
        );
        let mut elements = Vec::new();
        book.draw(
            &mut elements,
            1000.0,
            700.0,
            2.0,
            (563.0, 331.0),
            &|spans, scale| {
                spans
                    .iter()
                    .map(|span| span.text.chars().count() as f32 * scale * 0.5)
                    .sum()
            },
        );
        assert!(elements.iter().any(|e| matches!(
            e,
            crate::renderer::pipelines::menu_overlay::MenuElement::Image {
                sprite:
                    crate::renderer::pipelines::menu_overlay::SpriteId::BookPageForwardHighlighted,
                ..
            }
        )));
        book.set_page(1);
        elements.clear();
        book.draw(
            &mut elements,
            1000.0,
            700.0,
            2.0,
            (417.0, 331.0),
            &|spans, scale| {
                spans
                    .iter()
                    .map(|span| span.text.chars().count() as f32 * scale * 0.5)
                    .sum()
            },
        );
        assert!(elements.iter().any(|e| matches!(
            e,
            crate::renderer::pipelines::menu_overlay::MenuElement::Image {
                sprite:
                    crate::renderer::pipelines::menu_overlay::SpriteId::BookPageBackwardHighlighted,
                ..
            }
        )));
    }

    #[test]
    fn raw_nbt_change_page_survives_azalea_draw_and_style_hit_testing() {
        use std::io::Cursor;

        use azalea_buf::AzBuf;
        use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

        use crate::chat_component::ClickEvent;
        use crate::renderer::pipelines::menu_overlay::MenuElement;
        use crate::ui::text::TextSpan;

        let mut click = NbtCompound::new();
        click.insert("action", "change_page");
        click.insert("page", 3_i32);
        let mut linked = NbtCompound::new();
        linked.insert("text", "Next page");
        linked.insert("click_event", click);
        let mut plain = NbtCompound::new();
        plain.insert("text", "Read ");
        let mut bytes = Vec::new();
        // BaseNbt::write_unnamed only accepts compounds; NbtTag::write emits
        // the unnamed list tag consumed by Azalea's read_optional_tag, not JSON.
        NbtTag::List(NbtList::Compound(vec![plain, linked])).write(&mut bytes);
        let page =
            azalea_chat::FormattedText::azalea_read(&mut Cursor::new(bytes.as_slice())).unwrap();
        let mut book = BookViewState::new(vec![page, "two".into(), "three".into(), "four".into()]);
        let mut elements = Vec::new();
        let width = |spans: &[TextSpan], scale: f32| -> f32 {
            spans
                .iter()
                .map(|span| span.text.chars().count() as f32 * scale * 0.5)
                .sum()
        };
        book.draw(&mut elements, 1000.0, 700.0, 2.0, (0.0, 0.0), &width);
        let (x, y, spans, scale) = elements
            .iter()
            .find_map(|element| match element {
                MenuElement::TextSpans {
                    x, y, spans, scale, ..
                } => Some((*x, *y, spans, *scale)),
                _ => None,
            })
            .expect("draw must emit the decoded book text");
        assert_eq!(
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>(),
            ["Read ", "Next page"]
        );
        let glyph_width = scale * 0.5;
        let link_x = x + width(&spans[..1], scale);
        let cursor = (link_x + glyph_width / 2.0, y + scale / 2.0);
        assert_eq!(cursor, (424.0, 72.0)); // Center of the drawn 'N', at GUI scale 2.
        for index in 0.."Next page".chars().count() {
            let center = (cursor.0 + index as f32 * glyph_width, cursor.1);
            assert_eq!(
                book.style_at(center).unwrap().click_event,
                Some(ClickEvent::ChangePage(3))
            );
        }
        // Plain text has a resolved style, but no click event, including the
        // space immediately adjacent to the link. Outside text has no style.
        for index in 0.."Read ".chars().count() {
            let center = (x + (index as f32 + 0.5) * glyph_width, cursor.1);
            assert_eq!(book.style_at(center).unwrap().click_event, None);
        }
        assert!(book.style_at((x - glyph_width / 2.0, cursor.1)).is_none());
        assert!(
            book.style_at((x + width(spans, scale) + glyph_width / 2.0, cursor.1))
                .is_none()
        );

        // Mirror in_game's positive, one-based target guard and use the same
        // BookViewState::set_page helper for clamping and hit-region invalidation.
        let change_page = |book: &mut BookViewState, target: i32| {
            if target > 0 && !book.pages.is_empty() {
                book.set_page((target as usize) - 1);
            }
        };
        if let Some(ClickEvent::ChangePage(target)) = book
            .style_at(cursor)
            .and_then(|style| style.click_event.clone())
        {
            change_page(&mut book, target);
        }
        assert_eq!(book.page, 2);
        assert!(book.hit_regions.is_empty());
        assert!(book.style_at(cursor).is_none());
        change_page(&mut book, 99);
        assert_eq!(book.page, 3);
        change_page(&mut book, 0);
        assert_eq!(book.page, 3);
    }

    #[test]
    fn edit_hit_actions_match_all_drawn_button_centers() {
        let book = super::BookEditState::new(0, vec![], String::new(), String::new());
        let mut elements = Vec::new();
        book.draw(&mut elements, 1000.0, 700.0, 2.0, (0.0, 0.0));
        let centers: Vec<_> = elements
            .iter()
            .filter_map(|element| match element {
                crate::renderer::pipelines::menu_overlay::MenuElement::Rect {
                    x, y, w, h, ..
                } if *w == 96.0 && *h == 40.0 => Some((x + w / 2.0, y + h / 2.0)),
                _ => None,
            })
            .collect();
        assert_eq!(centers.len(), 4);
        for (index, center) in centers.iter().copied().enumerate() {
            assert_eq!(
                super::edit_hit_action(center, 1000.0, 700.0, 2.0),
                Some(index)
            );
        }
        assert_eq!(
            super::view_hit_action(centers[2], 1000.0, 700.0, 2.0, 0, 2),
            None
        );
    }

    #[test]
    fn book_view_page_navigation_stays_within_pages() {
        let mut book = BookViewState::new(
            vec!["one", "two"]
                .into_iter()
                .map(azalea_chat::FormattedText::from)
                .collect(),
        );
        book.navigate(0);
        assert_eq!(book.page, 0);
        book.navigate(1);
        book.navigate(1);
        assert_eq!(book.page, 1);
        book.navigate(0);
        assert_eq!(book.page, 0);
    }

    #[test]
    fn held_book_click_advances_only_once_until_next_press() {
        let mut book = BookViewState::new(
            vec!["one", "two", "three"]
                .into_iter()
                .map(azalea_chat::FormattedText::from)
                .collect(),
        );
        let mut input = InputState::released();
        let cursor = (563.0, 331.0);
        input.on_mouse_button(MouseButton::Left, ElementState::Pressed);

        let navigate = |input: &mut InputState, book: &mut BookViewState| {
            if input.left_just_pressed() {
                if let Some(index @ 0..=1) =
                    view_hit_action(cursor, 1000.0, 700.0, 2.0, book.page, book.pages.len())
                {
                    book.navigate(index);
                }
                input.consume_left_just_pressed();
            }
        };
        navigate(&mut input, &mut book);
        assert_eq!(book.page, 1);
        for _ in 0..3 {
            navigate(&mut input, &mut book);
            assert_eq!(book.page, 1);
        }
        input.on_mouse_button(MouseButton::Left, ElementState::Released);
        input.on_mouse_button(MouseButton::Left, ElementState::Pressed);
        navigate(&mut input, &mut book);
        assert_eq!(book.page, 2);
    }
}
