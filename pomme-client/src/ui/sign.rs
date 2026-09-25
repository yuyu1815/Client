use azalea_core::position::BlockPos;
use winit::keyboard::KeyCode;

use crate::ui::text_edit::{SystemClipboard, TextFieldState, TextInputEvent};

/// Client-only editing state; authorization remains entirely server-side.
pub struct SignEditState {
    pub pos: BlockPos,
    pub is_front_text: bool,
    fields: [TextFieldState; 4],
    selected: usize,
}

impl SignEditState {
    pub fn new(pos: BlockPos, is_front_text: bool, lines: [String; 4]) -> Self {
        let width = |s: &str| s.chars().count() as f32 * 6.0;
        let fields = std::array::from_fn(|i| {
            let mut field = TextFieldState::new(384);
            field.set_value(&lines[i], 90.0, &width);
            field
        });
        Self {
            pos,
            is_front_text,
            fields,
            selected: 0,
        }
    }

    pub fn lines(&self) -> [String; 4] {
        std::array::from_fn(|i| self.fields[i].value().to_owned())
    }

    /// Enter/Up/Down selects one of the four fixed lines. Other text editing
    /// reuses the shared EditBox implementation and rejects input past 90px.
    pub fn input(&mut self, events: &[TextInputEvent], width: &dyn Fn(&str) -> f32) {
        let mut clipboard = SystemClipboard;
        for event in events {
            if let TextInputEvent::Key { code, .. } = event {
                match code {
                    KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::ArrowDown => {
                        self.selected = (self.selected + 1).min(3);
                        continue;
                    }
                    KeyCode::ArrowUp => {
                        self.selected = self.selected.saturating_sub(1);
                        continue;
                    }
                    _ => {}
                }
            }
            let field = &mut self.fields[self.selected];
            let previous = field.value().to_owned();
            match event {
                TextInputEvent::Key { code, mods } => {
                    field.key_pressed(*code, mods, &mut clipboard, 90.0, width);
                }
                TextInputEvent::Char(ch) => {
                    field.char_typed(*ch, 90.0, width);
                }
            }
            if width(field.value()) > 90.0 {
                field.set_value(&previous, 90.0, width);
            }
        }
    }

    pub fn draw(
        &self,
        elements: &mut Vec<crate::renderer::pipelines::menu_overlay::MenuElement>,
        sw: f32,
        sh: f32,
        gs: f32,
    ) {
        use crate::renderer::pipelines::menu_overlay::MenuElement;
        let x = (sw - 220.0 * gs) / 2.0;
        let y = (sh - 150.0 * gs) / 2.0;
        elements.push(MenuElement::Rect {
            x,
            y,
            w: 220.0 * gs,
            h: 150.0 * gs,
            corner_radius: 3.0,
            color: [0.08, 0.08, 0.08, 0.96],
        });
        let scale = crate::ui::common::FONT_SIZE * gs;
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: y + 10.0 * gs,
            text: "Edit Sign".into(),
            scale,
            color: crate::ui::common::WHITE,
            centered: true,
        });
        for i in 0..4 {
            let yy = y + (38.0 + i as f32 * 22.0) * gs;
            elements.push(MenuElement::Rect {
                x: x + 22.0 * gs,
                y: yy - 2.0 * gs,
                w: 176.0 * gs,
                h: 18.0 * gs,
                corner_radius: 0.0,
                color: if self.selected == i {
                    [0.30, 0.30, 0.30, 1.0]
                } else {
                    [0.18, 0.18, 0.18, 1.0]
                },
            });
            elements.push(MenuElement::Text {
                x: x + 28.0 * gs,
                y: yy,
                text: format!(
                    "{}{}",
                    self.fields[i].value(),
                    if self.selected == i { "_" } else { "" }
                ),
                scale,
                color: crate::ui::common::WHITE,
                centered: false,
            });
        }
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: y + 132.0 * gs,
            text: "Done     Esc: Done".into(),
            scale,
            color: crate::ui::common::WHITE,
            centered: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::text_edit::KeyMods;

    #[test]
    fn sign_editor_selects_four_lines_and_preserves_face_and_position() {
        let pos = BlockPos { x: -2, y: 63, z: 9 };
        let mut edit =
            SignEditState::new(pos, false, ["a".into(), "b".into(), "c".into(), "d".into()]);
        let mods = KeyMods {
            shift: false,
            ctrl: false,
            alt: false,
            super_key: false,
        };
        edit.input(
            &[TextInputEvent::Key {
                code: KeyCode::Enter,
                mods,
            }],
            &|s| s.len() as f32 * 6.0,
        );
        assert_eq!(edit.lines(), ["a", "b", "c", "d"]);
        assert_eq!(edit.pos, pos);
        assert!(!edit.is_front_text);
    }
}
