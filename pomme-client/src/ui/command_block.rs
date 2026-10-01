use azalea_core::position::BlockPos;
use simdnbt::owned::{NbtCompound, NbtTag};
use winit::keyboard::KeyCode;

use crate::renderer::pipelines::menu_overlay::MenuElement;
use crate::ui::text_edit::{SystemClipboard, TextFieldState, TextInputEvent};

pub(crate) fn done_rect(sw: f32, sh: f32, gs: f32) -> [f32; 4] {
    [
        (sw - 200.0 * gs) / 2.0,
        (sh - 200.0 * gs) / 2.0 + 170.0 * gs,
        200.0 * gs,
        20.0 * gs,
    ]
}
pub(crate) fn cancel_rect(sw: f32, sh: f32, gs: f32) -> [f32; 4] {
    [
        (sw - 200.0 * gs) / 2.0,
        (sh - 200.0 * gs) / 2.0 + 145.0 * gs,
        200.0 * gs,
        20.0 * gs,
    ]
}
fn string(nbt: &NbtCompound, key: &str) -> String {
    nbt.get(key)
        .and_then(NbtTag::string)
        .map(|s| s.to_str().into_owned())
        .unwrap_or_default()
}
fn boolean(nbt: &NbtCompound, key: &str) -> bool {
    matches!(nbt.get(key), Some(NbtTag::Byte(v)) if *v != 0)
}

pub struct CommandBlockEditState {
    pub pos: BlockPos,
    command: TextFieldState,
    /// Protocol mode: sequence=0, auto=1, redstone=2.
    mode: u32,
    track_output: bool,
    conditional: bool,
    automatic: bool,
    last_output: String,
}
impl CommandBlockEditState {
    pub fn new(pos: BlockPos, id: &str, conditional: bool, nbt: &NbtCompound) -> Self {
        let mode = match id.strip_prefix("minecraft:").unwrap_or(id) {
            "chain_command_block" => 0,
            "repeating_command_block" => 1,
            _ => 2,
        };
        let mut command = TextFieldState::new(32_767);
        command.set_value(&string(nbt, "Command"), f32::MAX, &|_| 0.0);
        command.set_focused(true);
        Self {
            pos,
            command,
            mode,
            track_output: boolean(nbt, "TrackOutput"),
            conditional,
            automatic: boolean(nbt, "auto"),
            last_output: string(nbt, "LastOutput"),
        }
    }
    pub fn input(&mut self, events: &[TextInputEvent], width: &dyn Fn(&str) -> f32) {
        let mut clipboard = SystemClipboard;
        for event in events {
            if matches!(
                event,
                TextInputEvent::Key {
                    code: KeyCode::Enter | KeyCode::NumpadEnter,
                    ..
                }
            ) {
                continue;
            }
            self.command.handle(event, &mut clipboard, 420.0, width);
        }
    }
    pub fn packet(&self) -> (String, u32, u8) {
        (
            self.command.value().to_owned(),
            self.mode,
            u8::from(self.track_output)
                | (u8::from(self.conditional) << 1)
                | (u8::from(self.automatic) << 2),
        )
    }
    pub fn toggle(&mut self, which: usize) {
        match which {
            0 => self.mode = (self.mode + 1) % 3,
            1 => self.conditional = !self.conditional,
            2 => self.automatic = !self.automatic,
            _ => self.track_output = !self.track_output,
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
        use crate::renderer::pipelines::menu_overlay::SpriteId;
        use crate::ui::common;
        use crate::ui::common::{FONT_SIZE, WHITE};
        let y = (sh - 200.0 * gs) / 2.0;
        common::push_overlay(elements, sw, sh, 0.65);
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: y + 10.0 * gs,
            text: crate::lang::ui("Edit Command Block", "コマンドブロックを編集").into(),
            scale: FONT_SIZE * gs,
            color: WHITE,
            centered: true,
        });
        elements.push(MenuElement::NineSlice {
            x: (sw - 440.0 * gs) / 2.0,
            y: y + 40.0 * gs,
            w: 440.0 * gs,
            h: 24.0 * gs,
            sprite: SpriteId::ButtonNormal,
            border: 3.0 * gs,
            tint: WHITE,
        });
        let info = self.command.render_info(420.0 * gs, true, width);
        let text = &self.command.value()[info.display_start..info.display_end];
        common::push_field_text(
            elements,
            &info,
            text,
            None,
            (sw - 420.0 * gs) / 2.0,
            y + 47.0 * gs,
            FONT_SIZE * gs,
            gs,
            gs,
            WHITE,
            None,
            &|s| width(s) * gs,
        );
        let labels = [
            format!(
                "Mode: {}",
                ["Chain", "Repeating", "Impulse"][self.mode as usize]
            ),
            format!("Conditional: {}", self.conditional),
            format!("Automatic: {}", self.automatic),
            format!("Track Output: {}", self.track_output),
        ];
        for (i, label) in labels.iter().enumerate() {
            elements.push(MenuElement::Text {
                x: sw / 2.0,
                y: y + (76.0 + i as f32 * 14.0) * gs,
                text: label.clone(),
                scale: FONT_SIZE * gs,
                color: WHITE,
                centered: true,
            });
        }
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: y + 137.0 * gs,
            text: format!(
                "Last Output: {}",
                self.last_output.chars().take(70).collect::<String>()
            ),
            scale: FONT_SIZE * gs,
            color: WHITE,
            centered: true,
        });
        for rect in [cancel_rect(sw, sh, gs), done_rect(sw, sh, gs)] {
            elements.push(MenuElement::NineSlice {
                x: rect[0],
                y: rect[1],
                w: rect[2],
                h: rect[3],
                sprite: SpriteId::ButtonNormal,
                border: 3.0 * gs,
                tint: WHITE,
            });
        }
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: y + 151.0 * gs,
            text: crate::lang::ui("Cancel", "取消").into(),
            scale: FONT_SIZE * gs,
            color: WHITE,
            centered: true,
        });
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: y + 176.0 * gs,
            text: crate::lang::ui("Done", "完了").into(),
            scale: FONT_SIZE * gs,
            color: WHITE,
            centered: true,
        });
    }
}
pub(crate) fn toggle_rect(sw: f32, sh: f32, gs: f32, index: usize) -> [f32; 4] {
    [
        (sw - 260.0 * gs) / 2.0,
        (sh - 200.0 * gs) / 2.0 + (70.0 + index as f32 * 14.0) * gs,
        260.0 * gs,
        14.0 * gs,
    ]
}
