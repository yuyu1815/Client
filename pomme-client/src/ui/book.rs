use crate::renderer::pipelines::menu_overlay::MenuElement;
use crate::ui::common;
use crate::ui::text_edit::{
    MultilineField, SystemClipboard, TextFieldRenderInfo, TextFieldState, TextInputEvent,
};

const PAGE_WIDTH: f32 = 114.0;
const PAGE_LINES: usize = 14;
const INK: [f32; 4] = [0.12, 0.10, 0.08, 1.0];

/// Client preview only; saved components are replaced by inventory sync.
pub struct BookEditState {
    pub slot: u32,
    pub pages: Vec<String>,
    pub page: usize,
    pub title: TextFieldState,
    pub author: String,
    pub title_focused: bool,
    field: MultilineField,
    original_pages: Vec<String>,
    signing: bool,
    dragging: bool,
    close_requested: bool,
}

impl BookEditState {
    pub fn new(slot: u32, mut pages: Vec<String>, title: String, author: String) -> Self {
        pages.truncate(100);
        let original_pages = trimmed_pages(&pages);
        if pages.is_empty() {
            pages.push(String::new());
        }
        let mut field = MultilineField::new(1024, None);
        field.set_width(PAGE_WIDTH, &legacy_width);
        field.set_value(&pages[0], &legacy_width);
        let mut title_field = TextFieldState::new(15);
        title_field.set_value(&title, PAGE_WIDTH, &legacy_width);
        Self {
            slot,
            pages,
            page: 0,
            title: title_field,
            author,
            title_focused: false,
            field,
            original_pages,
            signing: false,
            dragging: false,
            close_requested: false,
        }
    }

    fn store_page(&mut self) {
        // Merely viewing an existing over-limit page must not rewrite it.
        self.pages[self.page] = self.field.value().to_owned();
    }

    pub fn navigate(&mut self, action: usize) {
        self.navigate_measured(action, &legacy_width);
    }

    pub fn navigate_measured(&mut self, action: usize, width: &dyn Fn(&str) -> f32) {
        if self.signing {
            return;
        }
        let page = match action {
            0 => self.page.saturating_sub(1),
            1 => (self.page + 1).min(99),
            _ => return,
        };
        if page == self.page {
            return;
        }
        if page == self.pages.len() {
            self.pages.push(String::new());
        }
        self.page = page;
        self.field.set_value(&self.pages[page], width);
        self.dragging = false;
    }

    pub fn is_signing(&self) -> bool {
        self.signing
    }
    pub fn should_close(&self) -> bool {
        self.close_requested
    }
    pub fn back(&mut self) {
        self.signing = false;
        self.title_focused = false;
        self.dragging = false;
        self.field.set_focused(true);
    }

    /// Original public entry point retained; gameplay supplies measured glyphs.
    pub fn input(
        &mut self,
        events: &[TextInputEvent],
        save: bool,
        sign: bool,
    ) -> Option<(u32, Vec<String>, Option<String>)> {
        self.input_measured(events, save, sign, &legacy_width)
    }

    pub fn input_measured(
        &mut self,
        events: &[TextInputEvent],
        save: bool,
        sign: bool,
        width: &dyn Fn(&str) -> f32,
    ) -> Option<(u32, Vec<String>, Option<String>)> {
        // Reflow even after resource fonts change without changing GUI scale.
        self.field.set_width(0.0, width);
        self.field.set_width(PAGE_WIDTH, width);
        let mut clipboard = SystemClipboard;
        for event in events {
            if let TextInputEvent::Key { code, .. } = event {
                use winit::keyboard::KeyCode;
                match code {
                    KeyCode::PageDown if !self.signing => {
                        self.navigate_measured(1, width);
                        continue;
                    }
                    KeyCode::PageUp if !self.signing => {
                        self.navigate_measured(0, width);
                        continue;
                    }
                    KeyCode::Escape if self.signing => {
                        self.back();
                        continue;
                    }
                    KeyCode::Enter | KeyCode::NumpadEnter if self.signing => continue,
                    _ => {}
                }
            }
            if self.signing {
                self.title.handle(event, &mut clipboard, PAGE_WIDTH, width);
            } else {
                let previous = self.field.value().to_owned();
                let cursor = self.field.cursor();
                let (a, b) = self.field.selection();
                let anchor = if cursor == a { b } else { a };
                self.field.handle(event, &mut clipboard, width);
                if self.field.value() != previous && self.field.line_count() > PAGE_LINES {
                    self.field.set_value(&previous, width);
                    self.field.set_selecting(false);
                    self.field.seek_cursor_to(anchor);
                    self.field.set_selecting(true);
                    self.field.seek_cursor_to(cursor);
                } else if self.field.value() != previous {
                    self.store_page();
                }
            }
        }
        if sign && !self.signing {
            self.signing = true;
            self.title_focused = true;
            self.title.set_focused(true);
            self.dragging = false;
            return None;
        }
        let title = self.title.value().trim();
        if self.signing && sign && !title.is_empty() {
            self.close_requested = true;
            return Some((
                self.slot,
                trimmed_pages(&self.pages),
                Some(title.to_owned()),
            ));
        }
        if save && !self.signing {
            self.close_requested = true;
            let pages = trimmed_pages(&self.pages);
            if pages != self.original_pages {
                return Some((self.slot, pages, None));
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mouse(
        &mut self,
        cursor: (f32, f32),
        sw: f32,
        sh: f32,
        gs: f32,
        pressed: bool,
        held: bool,
        shift: bool,
        width: &dyn Fn(&str) -> f32,
    ) {
        let layout = BookViewLayout::new(sw, sh, gs, self.page, self.pages.len());
        let x = layout.x + 36.0 * gs;
        let y = if self.signing {
            layout.y + 48.0 * gs
        } else {
            layout.y + 30.0 * gs
        };
        if pressed {
            self.dragging = common::hit_test(
                cursor,
                [
                    x,
                    y,
                    PAGE_WIDTH * gs,
                    if self.signing { 12.0 * gs } else { 126.0 * gs },
                ],
            );
        }
        if self.dragging && (pressed || held) {
            let rel_x = (cursor.0 - x) / gs;
            if self.signing {
                let pos = self.title.pos_from_click(rel_x, PAGE_WIDTH, width);
                if pressed {
                    self.title.on_click(pos, shift, PAGE_WIDTH, width);
                } else {
                    self.title.on_drag(pos, PAGE_WIDTH, width);
                }
            } else {
                self.field.set_selecting(if pressed { shift } else { true });
                self.field
                    .seek_cursor_to_point(rel_x, (cursor.1 - y) / gs, 9.0, width);
            }
        }
        if !held {
            self.dragging = false;
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
        self.draw_measured(elements, sw, sh, gs, cursor, &legacy_width);
    }

    pub fn draw_measured(
        &self,
        elements: &mut Vec<MenuElement>,
        sw: f32,
        sh: f32,
        gs: f32,
        cursor: (f32, f32),
        width: &dyn Fn(&str) -> f32,
    ) {
        use crate::renderer::pipelines::menu_overlay::SpriteId;
        let layout = BookViewLayout::new(sw, sh, gs, self.page, self.pages.len());
        elements.push(MenuElement::CroppedImage {
            x: layout.x,
            y: layout.y,
            w: layout.w,
            h: layout.h,
            sprite: SpriteId::BookBackground,
            uv_range: [0.0, 0.0, 0.75, 0.75],
            tint: [1.0; 4],
        });
        let scale = common::FONT_SIZE * gs;
        let x = layout.x + 36.0 * gs;
        let text = |elements: &mut Vec<MenuElement>, y: f32, value: String| {
            elements.push(MenuElement::Text {
                x,
                y: layout.y + y * gs,
                text: value,
                scale,
                color: INK,
                centered: false,
            });
        };
        if self.signing {
            text(elements, 30.0, "Enter Book Title:".into());
            let info = self.title.render_info(PAGE_WIDTH, true, width);
            let shown = &self.title.value()[info.display_start..info.display_end];
            common::push_field_text(
                elements,
                &info,
                shown,
                Some(&[crate::ui::text::TextSpan::new(shown.into(), INK)]),
                x,
                layout.y + 48.0 * gs,
                scale,
                gs,
                gs,
                INK,
                None,
                &|s| width(s) * gs,
            );
            text(elements, 66.0, format!("by {}", self.author));
            let warning = "Once you sign the book, it cannot be edited again.";
            for (i, (a, b)) in crate::ui::text_edit::split_lines(warning, PAGE_WIDTH, width)
                .into_iter()
                .enumerate()
            {
                text(elements, 90.0 + i as f32 * 9.0, warning[a..b].into());
            }
        } else {
            text(
                elements,
                10.0,
                format!("{}/{}", self.page + 1, self.pages.len()),
            );
            let (sel_a, sel_b) = self.field.selection();
            for (i, &(a, b)) in self.field.lines().iter().take(PAGE_LINES).enumerate() {
                let shown = &self.field.value()[a..b];
                let start = sel_a.max(a).min(b) - a;
                let end = sel_b.max(a).min(b) - a;
                let info = TextFieldRenderInfo {
                    display_start: a,
                    display_end: b,
                    caret_byte: self.field.cursor().clamp(a, b) - a,
                    caret_visible: i == self.field.line_at_cursor() && self.field.caret_visible(),
                    selection: (start < end).then_some((start, end)),
                    insert_mode: true,
                };
                common::push_field_text(
                    elements,
                    &info,
                    shown,
                    Some(&[crate::ui::text::TextSpan::new(shown.into(), INK)]),
                    x,
                    layout.y + (30.0 + i as f32 * 9.0) * gs,
                    scale,
                    gs,
                    gs,
                    INK,
                    None,
                    &|s| width(s) * gs,
                );
            }
        }
        for (i, rect) in edit_controls(sw, sh, gs).into_iter().enumerate() {
            let enabled = match i {
                0 => !self.signing && self.page > 0,
                1 => !self.signing && self.page < 99,
                3 => !self.signing || !self.title.value().trim().is_empty(),
                _ => true,
            };
            let label = match (self.signing, i) {
                (true, 2) => "Back",
                (true, 3) => "Sign and Close",
                (_, 0) => "Prev",
                (_, 1) => "Next",
                (_, 2) => "Done",
                _ => "Sign",
            };
            if self.signing && i < 2 {
                continue;
            }
            common::push_button(
                elements, cursor, rect[0], rect[1], rect[2], rect[3], gs, scale, label, enabled,
            );
        }
    }
}

fn trimmed_pages(pages: &[String]) -> Vec<String> {
    let end = pages
        .iter()
        .rposition(|page| !page.is_empty())
        .map_or(0, |i| i + 1);
    pages[..end].to_vec()
}

// ponytail: legacy wrappers use mono metrics; glyph-aware callers use
// *_measured.
fn legacy_width(s: &str) -> f32 {
    s.chars().count() as f32 * 6.0
}

fn edit_controls(sw: f32, sh: f32, gs: f32) -> [[f32; 4]; 4] {
    let layout = BookViewLayout::new(sw, sh, gs, 0, 1);
    [
        layout.previous,
        layout.next,
        [
            sw / 2.0 + 2.0 * gs,
            layout.y + 194.0 * gs,
            98.0 * gs,
            20.0 * gs,
        ],
        [
            sw / 2.0 - 100.0 * gs,
            layout.y + 194.0 * gs,
            98.0 * gs,
            20.0 * gs,
        ],
    ]
}

/// Read-only book opened by the server's OpenBook packet.
pub struct BookViewState {
    pub pages: Vec<azalea_chat::FormattedText>,
    pub page: usize,
    hit_regions: Vec<crate::ui::chat::StyleHitRegion>,
    rich_pages: Option<Vec<crate::chat_component::Component>>,
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
            rich_pages: None,
        }
    }

    /// Raw decoded pages retain hover/custom styles that Azalea discards.
    pub fn from_components(mut pages: Vec<crate::chat_component::Component>) -> Self {
        if pages.is_empty() {
            pages.push(crate::chat_component::Component::text(""));
        }
        let mut book = Self::new(
            pages
                .iter()
                .map(|page| {
                    azalea_chat::FormattedText::from(
                        crate::ui::text::format_component_spans(page, INK)
                            .into_iter()
                            .map(|span| span.text)
                            .collect::<String>(),
                    )
                })
                .collect(),
        );
        book.rich_pages = Some(pages);
        book
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
        let converted = serde_json::to_value(&self.pages[self.page])
            .ok()
            .and_then(|value| crate::chat_component::Component::from_value(&value).ok());
        let component = self
            .rich_pages
            .as_ref()
            .and_then(|pages| pages.get(self.page))
            .or(converted.as_ref());
        let spans = component
            .map(|component| crate::ui::text::format_component_spans(component, INK))
            .unwrap_or_else(|| {
                crate::ui::text::format_book_text_spans(&self.pages[self.page], INK)
            });
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

    #[allow(clippy::too_many_arguments)]
    pub fn draw_hover(
        &self,
        elements: &mut Vec<MenuElement>,
        cursor: (f32, f32),
        sw: f32,
        sh: f32,
        gs: f32,
        advanced: bool,
        width: &dyn Fn(&[crate::ui::text::TextSpan]) -> f32,
    ) {
        use crate::chat_component::{Component, HoverEvent};
        let Some(style) = self.style_at(cursor) else {
            return;
        };
        let Some(hover) = style.hover_event.as_ref() else {
            return;
        };
        let lines = match hover {
            HoverEvent::Text(component) => crate::ui::chat::wrapped_tooltip_lines(
                component,
                ((sw / gs).ceil() / 2.0).floor().max(200.0),
                width,
            ),
            HoverEvent::Item(value) => crate::ui::chat::item_tooltip_lines(value, None, advanced),
            HoverEvent::Entity(value) if advanced => {
                let mut lines = Vec::new();
                if let Some(name) = value
                    .get("name")
                    .and_then(|name| Component::from_value(name).ok())
                {
                    lines.extend(crate::ui::chat::component_tooltip_lines(&name));
                }
                if let Some(id) = value.get("id").and_then(serde_json::Value::as_str) {
                    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
                    let kind = Component::translate(
                        format!("entity.{namespace}.{}", path.replace('/', ".")),
                        Vec::new(),
                    );
                    lines.extend(crate::ui::chat::component_tooltip_lines(
                        &Component::translate(
                            "gui.entity_tooltip.type",
                            vec![crate::chat_component::Argument::Component(Box::new(kind))],
                        ),
                    ));
                }
                if let Some(uuid) = value.get("uuid").and_then(serde_json::Value::as_str) {
                    lines.push(crate::renderer::pipelines::menu_overlay::TooltipLine::new(
                        uuid.to_owned(),
                        common::WHITE,
                    ));
                }
                lines
            }
            HoverEvent::Entity(_) => Vec::new(),
        };
        if !lines.is_empty() {
            common::push_tooltip_lines(elements, cursor, sw, sh, gs, lines);
        }
    }

    pub fn key(&mut self, code: winit::keyboard::KeyCode) -> bool {
        match code {
            winit::keyboard::KeyCode::PageUp => self.navigate(0),
            winit::keyboard::KeyCode::PageDown => self.navigate(1),
            _ => return false,
        }
        true
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
    edit_controls(sw, sh, gs)
        .into_iter()
        .position(|rect| common::hit_test(cursor, rect))
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
    fn editor_limits_mouse_selection_rejection_and_sign_back_save() {
        use super::BookEditState;
        use crate::ui::text_edit::TextInputEvent;
        let width = |s: &str| {
            s.chars()
                .map(|c| if c == 'W' { 8.0 } else { 2.0 })
                .sum::<f32>()
        };
        let mut book = BookEditState::new(40, vec!["WiWi".into()], String::new(), "Author".into());
        book.input_measured(&[], false, false, &width);
        let layout = super::BookViewLayout::new(320.0, 240.0, 1.0, 0, 1);
        let origin = (layout.x + 36.0, layout.y + 30.0);
        book.mouse(origin, 320.0, 240.0, 1.0, true, true, false, &width);
        assert_eq!(book.field.cursor(), 0);
        book.mouse(
            (origin.0 + 10.0, origin.1),
            320.0,
            240.0,
            1.0,
            false,
            true,
            false,
            &width,
        );
        assert_eq!(book.field.selection(), (0, 2));
        let mut elements = Vec::new();
        book.draw_measured(&mut elements, 320.0, 240.0, 1.0, origin, &width);
        assert!(elements.iter().any(|e| matches!(e, crate::renderer::pipelines::menu_overlay::MenuElement::Rect { color, w, .. } if *color == crate::ui::common::FIELD_SELECTION && *w == 10.0)));
        assert!(elements.iter().any(|e| matches!(e, crate::renderer::pipelines::menu_overlay::MenuElement::Rect { color, w, .. } if *color == super::INK && *w == 1.0)));
        // Overflowing replacement is atomic, including the selection anchor.
        book.input_measured(
            &[TextInputEvent::Commit("\n".repeat(14))],
            false,
            false,
            &width,
        );
        assert_eq!(book.field.value(), "WiWi");
        assert_eq!(book.field.selection(), (0, 2));
        book.input_measured(
            &[TextInputEvent::Commit("abc".into())],
            false,
            false,
            &width,
        );
        assert_eq!(book.field.value(), "abcWi");
        book.navigate_measured(1, &width); // empty trailing page
        assert!(book.input_measured(&[], false, true, &width).is_none());
        assert!(book.is_signing());
        assert!(
            book.input_measured(&[TextInputEvent::Commit("   ".into())], false, true, &width)
                .is_none()
        );
        book.back();
        assert!(!book.is_signing());
        book.navigate_measured(0, &width);
        assert_eq!(book.field.value(), "abcWi");
        book.title.set_value("  Title  ", 114.0, &width);
        assert!(book.input_measured(&[], false, true, &width).is_none());
        let result = book.input_measured(&[], false, true, &width).unwrap();
        assert_eq!(result, (40, vec!["abcWi".into()], Some("Title".into())));
        assert!(book.should_close());

        let mut unchanged = BookEditState::new(
            0,
            vec!["original".into(), String::new()],
            String::new(),
            String::new(),
        );
        assert!(unchanged.input_measured(&[], true, false, &width).is_none());
        assert!(unchanged.should_close());
        let mut limited = BookEditState::new(0, vec![], String::new(), String::new());
        let zero_width = |_: &str| 0.0;
        limited.input_measured(
            &[TextInputEvent::Commit("😀".repeat(513))],
            false,
            false,
            &zero_width,
        );
        assert_eq!(limited.field.value().encode_utf16().count(), 1024);
        let before = limited.field.value().to_owned();
        limited.input_measured(&[TextInputEvent::Char('x')], false, false, &zero_width);
        assert_eq!(limited.field.value(), before);
        for _ in 0..110 {
            limited.navigate_measured(1, &zero_width);
        }
        assert_eq!(limited.page, 99);
        assert_eq!(limited.pages.len(), 100);
        let saved = limited
            .input_measured(&[], true, false, &zero_width)
            .unwrap();
        assert_eq!(saved.1.len(), 1); // all appended empty pages erased
        limited.input_measured(&[], false, true, &width);
        limited.input_measured(
            &[TextInputEvent::Commit("😀".repeat(8))],
            false,
            false,
            &width,
        );
        assert_eq!(limited.title.value().encode_utf16().count(), 14); // 15-unit UI limit
    }

    #[test]
    fn raw_component_book_hover_and_click_styles_survive_navigation() {
        let page = crate::chat_component::Component::from_value(&serde_json::json!({
            "text": "Link", "click_event": {"action": "open_url", "url": "https://example.com"},
            "hover_event": {"action": "show_text", "value": {"text": "Tooltip", "color": "red"}}
        }))
        .unwrap();
        let mut book = BookViewState::from_components(vec![
            page,
            crate::chat_component::Component::text("two"),
        ]);
        let mut elements = Vec::new();
        let width = |spans: &[crate::ui::text::TextSpan], scale: f32| {
            spans
                .iter()
                .map(|s| s.text.chars().count() as f32 * scale * 0.5)
                .sum()
        };
        book.draw(&mut elements, 320.0, 240.0, 1.0, (0.0, 0.0), &width);
        let cursor = (102.0, 36.0);
        assert!(matches!(
            book.style_at(cursor).unwrap().click_event,
            Some(crate::chat_component::ClickEvent::OpenUrl(_))
        ));
        book.draw_hover(&mut elements, cursor, 320.0, 240.0, 1.0, false, &|spans| {
            width(spans, crate::ui::common::FONT_SIZE)
        });
        assert!(elements.iter().any(|element| matches!(element, crate::renderer::pipelines::menu_overlay::MenuElement::TooltipLines { lines, .. } if lines[0].spans[0].text == "Tooltip")));
        assert!(book.key(winit::keyboard::KeyCode::PageDown));
        assert_eq!(book.page, 1);
        assert!(book.style_at(cursor).is_none());
        assert!(book.key(winit::keyboard::KeyCode::PageUp));
        assert_eq!(book.page, 0);
    }

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
                crate::renderer::pipelines::menu_overlay::MenuElement::NineSlice {
                    x,
                    y,
                    w,
                    h,
                    ..
                } => Some((x + w / 2.0, y + h / 2.0)),
                _ => None,
            })
            .collect();
        assert_eq!(centers.len(), 4);
        for (index, center) in centers.into_iter().enumerate() {
            assert_eq!(
                super::edit_hit_action(center, 1000.0, 700.0, 2.0),
                Some(index)
            );
        }
        // Hit rectangles share the geometry used to draw both modes.
        for (index, [x, y, w, h]) in super::edit_controls(1000.0, 700.0, 2.0)
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                super::edit_hit_action((x + w / 2.0, y + h / 2.0), 1000.0, 700.0, 2.0),
                Some(index)
            );
        }
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
