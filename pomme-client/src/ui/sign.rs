use azalea_core::position::BlockPos;
use winit::keyboard::KeyCode;

use crate::renderer::pipelines::menu_overlay::{MenuElement, SpriteId};
use crate::ui::text_edit::{SystemClipboard, TextFieldState, TextInputEvent};
use crate::world::block_entity::{sign_text_colors, sign_text_size};

/// Shared measured centering, in the caller's font-pixel coordinate system.
pub(crate) fn centered_line_x(center: f32, measured_width: f32) -> f32 {
    center - measured_width / 2.0
}

pub(crate) fn done_rect(sw: f32, sh: f32, gs: f32) -> [f32; 4] {
    [
        (sw - 200.0 * gs) / 2.0,
        (sh - 200.0 * gs) / 2.0 + 170.0 * gs,
        200.0 * gs,
        20.0 * gs,
    ]
}

/// Client-only editing state; authorization and stored NBT remain server-owned.
pub struct SignEditState {
    pub pos: BlockPos,
    pub is_front_text: bool,
    fields: [TextFieldState; 4],
    selected: usize,
    hanging: bool,
    wood: u8,
    wall: bool,
    dye: [f32; 3],
    glowing: bool,
}

impl SignEditState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pos: BlockPos,
        is_front_text: bool,
        lines: [String; 4],
        hanging: bool,
        wood: u8,
        wall: bool,
        style: ([f32; 3], bool),
        width: &dyn Fn(&str) -> f32,
    ) -> Self {
        let fields = std::array::from_fn(|i| {
            let mut field = TextFieldState::new(384);
            // Loading existing server text does not truncate it to our input limit.
            field.set_value(&lines[i], f32::MAX, width);
            field
        });
        Self {
            pos,
            is_front_text,
            fields,
            selected: 0,
            hanging,
            wood,
            wall,
            dye: style.0,
            glowing: style.1,
        }
    }

    pub fn lines(&self) -> [String; 4] {
        std::array::from_fn(|i| self.fields[i].value().to_owned())
    }

    /// Used only by world extraction: overlay precisely this position and face.
    pub fn preview_lines(
        &self,
        pos: BlockPos,
        front: bool,
        stored: Option<&[String; 4]>,
    ) -> [String; 4] {
        if self.pos == pos && self.is_front_text == front {
            self.lines()
        } else {
            stored.cloned().unwrap_or_default()
        }
    }

    pub fn input(&mut self, events: &[TextInputEvent], width: &dyn Fn(&str) -> f32) {
        let (limit, _) = sign_text_size(self.hanging);
        let mut clipboard = SystemClipboard;
        for event in events {
            if let TextInputEvent::Key { code, .. } = event {
                match code {
                    KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::ArrowDown => {
                        self.selected = (self.selected + 1) % 4;
                        self.fields[self.selected].set_focused(true);
                        continue;
                    }
                    KeyCode::ArrowUp => {
                        self.selected = (self.selected + 3) % 4;
                        self.fields[self.selected].set_focused(true);
                        continue;
                    }
                    _ => {}
                }
            }
            let field = &mut self.fields[self.selected];
            let previous = field.value().to_owned();
            let cursor = field.cursor();
            let selection = field.selection_range();
            let highlight = if cursor == selection.start {
                selection.end
            } else {
                selection.start
            };
            field.handle(event, &mut clipboard, f32::MAX, width);
            if field.value() != previous && width(field.value()) > limit {
                field.set_value(&previous, f32::MAX, width);
                field.move_cursor_to(highlight, false, f32::MAX, width);
                field.move_cursor_to(cursor, true, f32::MAX, width);
            }
        }
    }

    pub fn draw(
        &self,
        elements: &mut Vec<MenuElement>,
        sw: f32,
        sh: f32,
        gs: f32,
        width: &dyn Fn(&str) -> f32,
    ) {
        use crate::ui::common::{self, FONT_SIZE, WHITE};
        let y = (sh - 200.0 * gs) / 2.0;
        let center_y = y + if self.hanging { 90.0 } else { 70.0 } * gs;
        common::push_overlay(elements, sw, sh, 0.5);
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: y + 10.0 * gs,
            text: if self.hanging {
                crate::lang::ui("Edit Hanging Sign", "吊り看板を編集")
            } else {
                crate::lang::ui("Edit Sign", "看板を編集")
            }
            .into(),
            scale: FONT_SIZE * gs,
            color: WHITE,
            centered: true,
        });
        // Official 26.2 GUI assets and background scales; wall signs omit the post.
        let (w, h, offset, background_scale) = if self.hanging {
            (16.0, 16.0, -49.0, 4.5)
        } else {
            (24.0, if self.wall { 12.0 } else { 26.0 }, -23.7, 3.9)
        };
        elements.push(MenuElement::CroppedImage {
            x: sw / 2.0 - w * background_scale * gs / 2.0,
            y: center_y + offset * gs,
            w: w * background_scale * gs,
            h: h * background_scale * gs,
            sprite: SpriteId::SignBoard {
                wood: self.wood,
                hanging: self.hanging,
            },
            uv_range: [
                0.0,
                0.0,
                1.0,
                if self.wall && !self.hanging {
                    12.0 / 26.0
                } else {
                    1.0
                },
            ],
            tint: WHITE,
        });
        let (_, line_height) = sign_text_size(self.hanging);
        let px = gs * if self.hanging { 1.0 } else { 0.9765628 };
        let (color, dark) = sign_text_colors(self.dye, self.glowing, 1.0);
        let rgba = |rgb: [f32; 3]| [rgb[0], rgb[1], rgb[2], 1.0];
        for (i, field) in self.fields.iter().enumerate() {
            // Render the whole measured line, rather than a left-aligned EditBox viewport.
            let info = field.render_info(f32::MAX, self.selected == i, width);
            let shown = &field.value()[info.display_start..info.display_end];
            let x = centered_line_x(sw / 2.0, width(shown) * px);
            let yy = center_y + (i as f32 * line_height - line_height * 2.0) * px;
            if self.glowing {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx != 0 || dy != 0 {
                            elements.push(MenuElement::TextFlat {
                                x: x + dx as f32 * px * 0.5,
                                y: yy + dy as f32 * px * 0.5,
                                text: shown.into(),
                                scale: FONT_SIZE * px,
                                color: rgba(dark),
                            });
                        }
                    }
                }
            }
            common::push_field_text(
                elements,
                &info,
                shown,
                Some(&[crate::ui::text::TextSpan::new(shown.into(), rgba(color))]),
                x,
                yy,
                FONT_SIZE * px,
                px,
                px,
                rgba(color),
                None,
                &|s| width(s) * px,
            );
        }
        let [x, yy, w, h] = done_rect(sw, sh, gs);
        elements.push(MenuElement::NineSlice {
            x,
            y: yy,
            w,
            h,
            sprite: SpriteId::ButtonNormal,
            border: 3.0 * gs,
            tint: WHITE,
        });
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: yy + 6.0 * gs,
            text: crate::lang::ui("Done", "完了").into(),
            scale: FONT_SIZE * gs,
            color: WHITE,
            centered: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use azalea_registry::builtin::BlockEntityKind;
    use simdnbt::owned::{NbtCompound, NbtList};

    use super::*;
    use crate::ui::text_edit::KeyMods;
    use crate::world::block_entity::StoredBlockEntity;

    fn editor(hanging: bool) -> SignEditState {
        SignEditState::new(
            BlockPos::new(-2, 63, 9),
            false,
            ["Wi".into(), "b".into(), "c".into(), "d".into()],
            hanging,
            0,
            false,
            ([29.0 / 255.0, 29.0 / 255.0, 33.0 / 255.0], false),
            &measure,
        )
    }
    fn measure(s: &str) -> f32 {
        s.chars().map(|ch| if ch == 'W' { 8.0 } else { 2.0 }).sum()
    }

    #[test]
    fn sign_editor_draw_centers_measured_lines_and_reuses_selection() {
        let mut edit = editor(false);
        edit.input(
            &[TextInputEvent::Key {
                code: KeyCode::KeyA,
                mods: KeyMods {
                    shift: false,
                    ctrl: true,
                    alt: false,
                    super_key: cfg!(target_os = "macos"),
                },
            }],
            &measure,
        );
        let mut elements = Vec::new();
        edit.draw(&mut elements, 320.0, 240.0, 1.0, &measure);
        let (x, scale) = elements
            .iter()
            .find_map(|e| match e {
                MenuElement::McText {
                    x,
                    spans,
                    scale,
                    shadow: false,
                    ..
                } if spans[0].text == "Wi" => Some((*x, *scale)),
                _ => None,
            })
            .unwrap();
        assert!(
            (x + measure("Wi") * scale / crate::ui::common::FONT_SIZE / 2.0 - 160.0).abs() < 1e-5
        );
        assert!(elements.iter().any(|e| matches!(e, MenuElement::Rect { color, w, .. } if *color == crate::ui::common::FIELD_SELECTION && *w > 0.0)));
        assert!(elements.iter().any(|e| matches!(
            e,
            MenuElement::CroppedImage {
                sprite: SpriteId::SignBoard { hanging: false, .. },
                ..
            }
        )));
    }

    #[test]
    fn edited_world_lines_match_only_target_face_and_leave_nbt_untouched() {
        let mut edit = editor(true);
        edit.input(&[TextInputEvent::Commit(" edited".into())], &measure);
        assert_eq!(edit.lines()[0], "Wi edited");
        let mut face = NbtCompound::new();
        face.insert("messages", NbtList::String(vec!["\"server\"".into()]));
        let mut nbt = NbtCompound::new();
        nbt.insert("front_text", face.clone());
        nbt.insert("back_text", face);
        let entity = StoredBlockEntity::new(BlockEntityKind::HangingSign, nbt.clone());
        assert_eq!(
            edit.preview_lines(edit.pos, false, entity.sign_back.as_ref()),
            edit.lines()
        );
        assert_eq!(
            edit.preview_lines(edit.pos, true, entity.sign_front.as_ref()),
            entity.sign_front.clone().unwrap()
        );
        assert_eq!(
            edit.preview_lines(BlockPos::new(0, 63, 9), false, entity.sign_back.as_ref()),
            entity.sign_back.clone().unwrap()
        );
        assert_eq!(entity.nbt, nbt);
        assert_eq!(entity.sign_back.as_ref().unwrap()[0], "server");
    }

    #[test]
    fn hanging_sign_input_uses_sixty_pixels_and_nine_pixel_rows() {
        assert_eq!(sign_text_size(true), (60.0, 9.0));
        assert_eq!(sign_text_size(false), (90.0, 10.0));
        for hanging in [false, true] {
            let mut edit = editor(hanging);
            edit.fields[0].set_value("", 90.0, &measure);
            edit.input(&[TextInputEvent::Commit("W".repeat(8))], &measure);
            assert_eq!(edit.lines()[0].len(), if hanging { 0 } else { 8 });
            edit.input(
                &[TextInputEvent::Key {
                    code: KeyCode::Enter,
                    mods: KeyMods {
                        shift: false,
                        ctrl: false,
                        alt: false,
                        super_key: false,
                    },
                }],
                &measure,
            );
            assert_eq!(edit.selected, 1);
            let mut elements = Vec::new();
            edit.draw(&mut elements, 320.0, 240.0, 1.0, &measure);
            let rows: Vec<f32> = elements
                .iter()
                .filter_map(|e| match e {
                    MenuElement::McText { y, .. } => Some(*y),
                    _ => None,
                })
                .collect();
            let px = if hanging { 1.0 } else { 0.9765628 };
            assert!((rows[1] - rows[0] - sign_text_size(hanging).1 * px).abs() < 1e-5);
            assert!(elements.iter().any(|e| matches!(e, MenuElement::CroppedImage { sprite: SpriteId::SignBoard { hanging: actual, .. }, .. } if *actual == hanging)));
        }
    }
}
