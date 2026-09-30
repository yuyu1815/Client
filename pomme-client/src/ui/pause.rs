use super::common;
use super::common::WHITE;
use crate::renderer::pipelines::menu_overlay::MenuElement;

const FULL_W: f32 = 204.0;
const HALF_W: f32 = 98.0;
const PADDING: f32 = 4.0;
const MENU_PADDING_TOP: f32 = 50.0;

/// Which pause screen is showing. The pause menu is a small stack:
/// `Main` -> `Benchmark` -> `ChunkLoader`.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PauseScreen {
    #[default]
    Main,
    Benchmark,
    ChunkLoader,
    /// F3+Esc: paused with no menu rendered (vanilla `pauseGame(true)`).
    Hidden,
}

#[derive(Clone, Copy)]
pub enum PauseAction {
    None,
    Resume,
    Disconnect,
    Options,
    ReportBugs,
    OpenBenchmark,
    StartFpsBenchmark,
    StartMovementRecording,
    StopMovementRecording,
    OpenChunkLoader,
    StartChunkLoad(u32),
    Back,
}

#[allow(clippy::too_many_arguments)]
pub fn build_pause_menu(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    clicked: bool,
    gs: f32,
    screen: PauseScreen,
    server_rd: u32,
    singleplayer: bool,
    recording: bool,
) -> PauseAction {
    match screen {
        // The F3+Esc pause renders nothing; update_game skips building it.
        PauseScreen::Hidden => PauseAction::None,
        PauseScreen::Main => build_main(
            elements,
            screen_w,
            screen_h,
            cursor,
            clicked,
            gs,
            singleplayer,
        ),
        PauseScreen::Benchmark => build_submenu(
            elements,
            screen_w,
            screen_h,
            cursor,
            clicked,
            gs,
            crate::lang::ui("Benchmark", "ベンチマーク"),
            None,
            &[
                (
                    crate::lang::ui("FPS / Frametime", "FPS / フレーム時間"),
                    PauseAction::StartFpsBenchmark,
                ),
                (
                    crate::lang::ui("Chunk Loader", "チャンク読み込み"),
                    PauseAction::OpenChunkLoader,
                ),
                (
                    if recording {
                        crate::lang::ui("Stop movement recording", "移動・行動記録を停止")
                    } else {
                        crate::lang::ui("Record movement / actions", "移動・行動記録を開始")
                    },
                    if recording {
                        PauseAction::StopMovementRecording
                    } else {
                        PauseAction::StartMovementRecording
                    },
                ),
                (crate::lang::ui("Back", "戻る"), PauseAction::Back),
            ],
        ),
        PauseScreen::ChunkLoader => {
            let subtitle = if server_rd > 0 {
                format!(
                    "{}: {server_rd}",
                    crate::lang::ui("Server render distance", "サーバーの描画距離")
                )
            } else {
                format!(
                    "{}: {}",
                    crate::lang::ui("Server render distance", "サーバーの描画距離"),
                    crate::lang::ui("unknown", "不明")
                )
            };
            build_submenu(
                elements,
                screen_w,
                screen_h,
                cursor,
                clicked,
                gs,
                crate::lang::ui("Chunk Loader", "チャンク読み込み"),
                Some(&subtitle),
                &[
                    (
                        crate::lang::ui("Render Distance 8", "描画距離 8"),
                        PauseAction::StartChunkLoad(8),
                    ),
                    (
                        crate::lang::ui("Render Distance 16", "描画距離 16"),
                        PauseAction::StartChunkLoad(16),
                    ),
                    (
                        crate::lang::ui("Render Distance 24", "描画距離 24"),
                        PauseAction::StartChunkLoad(24),
                    ),
                    (
                        crate::lang::ui("Render Distance 32", "描画距離 32"),
                        PauseAction::StartChunkLoad(32),
                    ),
                    (crate::lang::ui("Back", "戻る"), PauseAction::Back),
                ],
            )
        }
    }
}

fn build_main(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    clicked: bool,
    gs: f32,
    singleplayer: bool,
) -> PauseAction {
    let mut action = PauseAction::None;
    let fs = common::FONT_SIZE * gs;

    common::push_overlay(elements, screen_w, screen_h, 0.47);

    let full_w = FULL_W * gs;
    let half_w = HALF_W * gs;
    let btn_h = common::BTN_H * gs;
    let pad = PADDING * gs;
    let top_pad = MENU_PADDING_TOP * gs;

    let grid_w = (half_w + pad) * 2.0 + pad * 2.0;
    let grid_h = (top_pad + btn_h) + 4.0 * (pad + btn_h);

    let grid_x = (screen_w - grid_w) / 2.0;
    let grid_y = (screen_h - grid_h) * 0.25;

    let col1_x = grid_x + pad;
    let col2_x = col1_x + half_w + pad * 2.0;
    let full_x = col1_x;

    let row_y = |row: u32| -> f32 { grid_y + top_pad + row as f32 * (btn_h + pad) };

    elements.push(MenuElement::Text {
        x: screen_w / 2.0,
        y: grid_y + 40.0 * gs - top_pad,
        text: crate::lang::ui("Game", "ゲームメニュー").into(),
        scale: fs,
        color: WHITE,
        centered: true,
    });

    if common::push_button(
        elements,
        cursor,
        full_x,
        row_y(0),
        full_w,
        btn_h,
        gs,
        fs,
        crate::lang::ui("Return to Game", "ゲームに戻る"),
        true,
    ) && clicked
    {
        action = PauseAction::Resume;
    }

    common::push_button(
        elements,
        cursor,
        col1_x,
        row_y(1),
        half_w,
        btn_h,
        gs,
        fs,
        crate::lang::ui("Advancements", "進捗"),
        false,
    );
    common::push_button(
        elements,
        cursor,
        col2_x,
        row_y(1),
        half_w,
        btn_h,
        gs,
        fs,
        crate::lang::ui("Statistics", "統計"),
        false,
    );

    common::push_button(
        elements,
        cursor,
        col1_x,
        row_y(2),
        half_w,
        btn_h,
        gs,
        fs,
        crate::lang::ui("Give Feedback", "フィードバックを送る"),
        false,
    );
    if common::push_button(
        elements,
        cursor,
        col2_x,
        row_y(2),
        half_w,
        btn_h,
        gs,
        fs,
        crate::lang::ui("Report Bugs", "バグを報告"),
        true,
    ) && clicked
    {
        action = PauseAction::ReportBugs;
    }

    if common::push_button(
        elements,
        cursor,
        col1_x,
        row_y(3),
        half_w,
        btn_h,
        gs,
        fs,
        crate::lang::ui("Options...", "設定..."),
        true,
    ) && clicked
    {
        action = PauseAction::Options;
    }
    if common::push_button(
        elements,
        cursor,
        col2_x,
        row_y(3),
        half_w,
        btn_h,
        gs,
        fs,
        crate::lang::ui("Benchmark", "ベンチマーク"),
        true,
    ) && clicked
    {
        action = PauseAction::OpenBenchmark;
    }

    // Vanilla `menu.returnToMenu` on a local server.
    let leave_label = if singleplayer {
        crate::lang::ui("Save and Quit to Title", "セーブしてタイトルへ戻る")
    } else {
        crate::lang::ui("Disconnect", "切断")
    };
    if common::push_button(
        elements,
        cursor,
        full_x,
        row_y(4),
        full_w,
        btn_h,
        gs,
        fs,
        leave_label,
        true,
    ) && clicked
    {
        action = PauseAction::Disconnect;
    }

    action
}

#[cfg(test)]
mod movement_tests {
    use super::*;
    #[test]
    fn benchmark_menu_record_start_stop_click_path() {
        for recording in [false, true] {
            let mut elements = Vec::new();
            build_pause_menu(
                &mut elements,
                800.0,
                600.0,
                (0.0, 0.0),
                false,
                1.0,
                PauseScreen::Benchmark,
                8,
                false,
                recording,
            );
            let cursor = elements
                .iter()
                .find_map(|element| match element {
                    MenuElement::Text { x, y, text, .. }
                        if text.contains("movement") || text.contains("移動・行動") =>
                    {
                        Some((*x, *y + 2.0))
                    }
                    _ => None,
                })
                .expect("recording button must be visible");
            let action = build_pause_menu(
                &mut Vec::new(),
                800.0,
                600.0,
                cursor,
                true,
                1.0,
                PauseScreen::Benchmark,
                8,
                false,
                recording,
            );
            assert!(matches!(
                (recording, action),
                (false, PauseAction::StartMovementRecording)
                    | (true, PauseAction::StopMovementRecording)
            ));
        }
    }
}

/// A simple centered column of full-width buttons under a title, used by the
/// benchmark sub-screens.
#[allow(clippy::too_many_arguments)]
fn build_submenu(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    clicked: bool,
    gs: f32,
    title: &str,
    subtitle: Option<&str>,
    items: &[(&str, PauseAction)],
) -> PauseAction {
    let mut action = PauseAction::None;
    let fs = common::FONT_SIZE * gs;

    common::push_overlay(elements, screen_w, screen_h, 0.47);

    let full_w = FULL_W * gs;
    let btn_h = common::BTN_H * gs;
    let pad = PADDING * gs;
    let top_pad = MENU_PADDING_TOP * gs;

    let n = items.len() as f32;
    let grid_h = top_pad + n * btn_h + (n - 1.0).max(0.0) * pad;
    let grid_y = (screen_h - grid_h) * 0.25;
    let x = (screen_w - full_w) / 2.0;

    let title_y = grid_y + 40.0 * gs - top_pad;
    elements.push(MenuElement::Text {
        x: screen_w / 2.0,
        y: title_y,
        text: title.into(),
        scale: fs,
        color: WHITE,
        centered: true,
    });
    if let Some(sub) = subtitle {
        elements.push(MenuElement::Text {
            x: screen_w / 2.0,
            y: title_y + fs * 1.4,
            text: sub.into(),
            scale: fs * 0.8,
            color: [0.7, 0.74, 0.8, 1.0],
            centered: true,
        });
    }

    for (i, (label, item_action)) in items.iter().enumerate() {
        let y = grid_y + top_pad + i as f32 * (btn_h + pad);
        if common::push_button(elements, cursor, x, y, full_w, btn_h, gs, fs, label, true)
            && clicked
        {
            action = *item_action;
        }
    }

    action
}
