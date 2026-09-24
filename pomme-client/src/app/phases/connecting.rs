use crate::app::TICK_RATE;
use crate::app::core::AppCore;
use crate::app::phases::in_game::{GameState, build_server_screens};
use crate::app::phases::{ConnectionPhase, Gfx, Panorama, draw_status};
use crate::net::connection::ConnectionHandle;
use crate::singleplayer::World;
use crate::ui::hud;

pub enum ConnectingUpdateResult {
    None,
    ManualDisconnect,
    Disconnected { reason: String },
    Transfer(crate::net::ServerTransfer),
    JoinGame,
}

#[expect(
    clippy::too_many_arguments,
    reason = "one parameter per field of the phase it updates"
)]
pub fn update_connecting(
    core: &mut AppCore,
    dt: f32,
    gfx: &mut Gfx,
    panorama: &mut Panorama,
    connect_phase: &mut ConnectionPhase,
    connection: &ConnectionHandle,
    game: &mut GameState,
    world: Option<&mut World>,
) -> ConnectingUpdateResult {
    // Polled before the network, so a server that failed to start reports its
    // own reason rather than the end of file its death also causes. The phase
    // stays `StartingWorld` until the connection reports in; vanilla shows one
    // screen from server start until terrain appears.
    if let Some(world) = world
        && let Err(reason) = world.poll()
    {
        return ConnectingUpdateResult::Disconnected { reason };
    }

    let disconnect_reason = core.drain_network_events(
        connection,
        Some(connect_phase),
        &mut gfx.renderer,
        &gfx.window,
        game,
    );
    if let Some(transfer) = game.pending_server_transfer.take() {
        return ConnectingUpdateResult::Transfer(transfer);
    }
    if let Some(reason) = disconnect_reason {
        return ConnectingUpdateResult::Disconnected { reason };
    }

    if matches!(connect_phase, ConnectionPhase::Loading) {
        game.mesh_dispatcher
            .set_camera_position(*game.player.position);
        game.drain_and_upload_meshes(&mut gfx.renderer);
        // Vanilla runs `ClientLevel.update()` every frame, loading screen
        // included; the load gate waits on the light this applies.
        game.update_light(core.menu.chunk_detail);

        // Vanilla keeps ticking behind the loading screen: the tracker advances
        // and every tick is still marked with `client_tick_end`, while
        // `LocalPlayer.tick` stays parked. Those ticks need a level, which
        // arrives with the login that also starts the tracker.
        if game.level_load.is_some() {
            core.tick_accumulator += dt;
            while core.tick_accumulator >= TICK_RATE {
                AppCore::tick_level_load(&gfx.renderer, connection, game);
                if game.client_loaded {
                    // Vanilla closes `LevelLoadingScreen` on the same tick that
                    // sends `player_loaded`, and the local player then ticks
                    // (and moves) later in it. Hand this tick to the game phase
                    // unspent so it plays out there.
                    return ConnectingUpdateResult::JoinGame;
                }
                AppCore::send_client_tick_end(connection);
                core.tick_accumulator -= TICK_RATE;
            }
        } else {
            core.tick_accumulator = 0.0;
        }
    }

    let status_text = match connect_phase {
        ConnectionPhase::StartingWorld | ConnectionPhase::Loading => "Loading terrain...",
        ConnectionPhase::Connecting => "Connecting to the server...",
    };

    if game.code_of_conduct.is_some() {
        if let Some(accept) = draw_code_of_conduct(core, dt, gfx, panorama, game) {
            if !accept {
                connection.packet_tx.decide_code_of_conduct(false);
                return ConnectingUpdateResult::ManualDisconnect;
            }
            connection.packet_tx.decide_code_of_conduct(true);
            game.code_of_conduct = None;
        }
    } else if game.dialog_open() {
        draw_server_dialog(core, dt, gfx, panorama, connection, game);
    } else if draw_status(core, dt, gfx, panorama, status_text, Some("Cancel")) {
        return ConnectingUpdateResult::ManualDisconnect;
    }

    ConnectingUpdateResult::None
}

/// Shows the server's complete notice with explicit, one-shot accept/decline.
fn draw_code_of_conduct(
    core: &mut AppCore,
    dt: f32,
    gfx: &mut Gfx,
    panorama: &mut Panorama,
    game: &mut GameState,
) -> Option<bool> {
    use crate::renderer::pipelines::menu_overlay::MenuElement;

    panorama.update(dt);
    let sw = gfx.renderer.screen_width() as f32;
    let sh = gfx.renderer.screen_height() as f32;
    let gs = hud::gui_scale(sw, sh, core.menu.gui_scale_setting);
    let fs = 8.0 * gs;
    let left = sw * 0.12;
    let panel_w = sw * 0.76;
    let panel_y = sh * 0.12;
    let panel_h = sh * 0.76;
    let mut elements = vec![
        MenuElement::Rect {
            x: left,
            y: panel_y,
            w: panel_w,
            h: panel_h,
            corner_radius: 5.0,
            color: [0.04, 0.04, 0.06, 0.96],
        },
        MenuElement::Text {
            x: sw / 2.0,
            y: panel_y + 12.0,
            text: "Code of Conduct".into(),
            scale: fs * 1.5,
            color: [1.0; 4],
            centered: true,
        },
        MenuElement::ScissorPush {
            x: left + 12.0,
            y: panel_y + 38.0,
            w: panel_w - 24.0,
            h: panel_h - 92.0,
        },
    ];
    let text = game.code_of_conduct.as_deref().unwrap_or_default();
    let max_width = panel_w - 28.0;
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && gfx.renderer.menu_text_width(&candidate, fs) > max_width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    let visible_lines = ((panel_h - 92.0) / (fs + 3.0)).floor().max(1.0) as usize;
    let scroll = core.input.consume_menu_scroll().round() as isize;
    game.code_of_conduct_scroll = if scroll < 0 {
        game.code_of_conduct_scroll
            .saturating_add((-scroll) as usize)
    } else {
        game.code_of_conduct_scroll.saturating_sub(scroll as usize)
    }
    .min(lines.len().saturating_sub(visible_lines));
    for (i, line) in lines
        .iter()
        .skip(game.code_of_conduct_scroll)
        .take(visible_lines)
        .enumerate()
    {
        elements.push(MenuElement::Text {
            x: left + 14.0,
            y: panel_y + 42.0 + i as f32 * (fs + 3.0),
            text: line.clone(),
            scale: fs,
            color: [0.92, 0.92, 0.92, 1.0],
            centered: false,
        });
    }
    elements.push(MenuElement::ScissorPop);
    let accept = [
        left + panel_w / 2.0 - 110.0,
        panel_y + panel_h - 40.0,
        100.0,
        24.0,
    ];
    let decline = [
        left + panel_w / 2.0 + 10.0,
        panel_y + panel_h - 40.0,
        100.0,
        24.0,
    ];
    let cursor = core.input.cursor_pos();
    for (rect, label, color) in [
        (accept, "Accept", [0.12, 0.42, 0.18, 1.0]),
        (decline, "Decline", [0.48, 0.14, 0.14, 1.0]),
    ] {
        elements.push(MenuElement::Rect {
            x: rect[0],
            y: rect[1],
            w: rect[2],
            h: rect[3],
            corner_radius: 3.0,
            color,
        });
        elements.push(MenuElement::Text {
            x: rect[0] + rect[2] / 2.0,
            y: rect[1] + 7.0,
            text: label.into(),
            scale: fs,
            color: [1.0; 4],
            centered: true,
        });
    }
    let clicked = core.input.left_just_pressed();
    let inside = |r: [f32; 4]| {
        cursor.0 >= r[0] && cursor.0 <= r[0] + r[2] && cursor.1 >= r[1] && cursor.1 <= r[1] + r[3]
    };
    let decision = if core.input.escape_pressed() || (clicked && inside(decline)) {
        Some(false)
    } else if core.input.enter_pressed() || (clicked && inside(accept)) {
        Some(true)
    } else {
        None
    };
    if let Err(e) =
        gfx.renderer
            .render_menu(&gfx.window, panorama.scroll(), 2.0, elements, cursor, false)
    {
        tracing::error!("Render error: {e}");
    }
    decision
}

/// A configuration-phase dialog, shown in place of the connect screen with
/// the confirm screen its links can open.
fn draw_server_dialog(
    core: &mut AppCore,
    dt: f32,
    gfx: &mut Gfx,
    panorama: &mut Panorama,
    connection: &ConnectionHandle,
    game: &mut GameState,
) {
    panorama.update(dt);

    let sw = gfx.renderer.screen_width() as f32;
    let sh = gfx.renderer.screen_height() as f32;
    let gs = hud::gui_scale(sw, sh, core.menu.gui_scale_setting);

    // A configuration-phase dialog can carry object glyphs, which load into
    // the same atlas the in-game text uses.
    core.sync_game_dynamic_atlas(game, &mut gfx.renderer, false);

    let mut elements = Vec::new();
    let text_events = core.input.drain_text_events();
    // The connecting screen runs no client ticks, so the dialog's own
    // timers fall back to wall time.
    build_server_screens(
        &mut elements,
        sw,
        sh,
        gs,
        core,
        gfx,
        connection,
        game,
        None,
        &text_events,
    );

    let cursor = core.input.cursor_pos();
    if let Err(e) =
        gfx.renderer
            .render_menu(&gfx.window, panorama.scroll(), 2.0, elements, cursor, false)
    {
        tracing::error!("Render error: {e}");
    }
}
