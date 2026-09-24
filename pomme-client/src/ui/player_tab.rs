//! Ported 1:1 from vanilla `PlayerTabOverlay.extractRenderState`
//! (reference/26.1/decompiled/.../PlayerTabOverlay.java:103-204).

use std::collections::{HashMap, HashSet};

use crate::entity::EntityStore;
use crate::player::tab_list::{TabList, TabListPlayer};
use crate::renderer::pipelines::menu_overlay::{MenuElement, SpriteId};
use crate::ui::common::FONT_SIZE;
use crate::ui::hud::Scoreboard;
use crate::ui::text::TextSpan;

#[path = "player_tab_float.rs"]
mod float_text;

const MAX_ROWS_PER_COL: usize = 20;
const HEAD_COL_W: i32 = 9;
const LINE_HEIGHT: i32 = 9;
const SPECTATOR_GAME_MODE: u8 = 3;
const HEART_SCORE_W: i32 = 90;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct HealthScore {
    last: i32,
    displayed: i32,
    blink_until: u64,
    last_update_tick: u64,
}

#[derive(Default)]
pub struct TabScoreState {
    visible: bool,
    health: HashMap<uuid::Uuid, HealthScore>,
}

impl TabScoreState {
    pub fn set_visible(&mut self, visible: bool) {
        if self.visible != visible {
            self.health.clear();
            self.visible = visible;
        }
    }
    fn prune(&mut self, listed: &HashSet<uuid::Uuid>) {
        self.health.retain(|uuid, _| listed.contains(uuid));
    }

    fn update(&mut self, uuid: uuid::Uuid, value: i32, tick: u64) -> HealthScore {
        let score = self.health.entry(uuid).or_insert(HealthScore {
            last: value,
            displayed: value,
            blink_until: 0,
            last_update_tick: tick,
        });
        if score.last != value {
            score.blink_until = tick.saturating_add(if value < score.last { 20 } else { 10 });
            score.last = value;
            score.last_update_tick = tick;
        }
        if tick.saturating_sub(score.last_update_tick) > 20 {
            score.displayed = score.last;
        }
        *score
    }
}
fn heart_blink(score: HealthScore, tick: u64) -> bool {
    score.blink_until > tick && (score.blink_until - tick) % 6 >= 3
}
fn heart_counts(current: i32, displayed: i32) -> (usize, usize) {
    let max = current.max(displayed);
    (
        ((i64::from(max).max(0) + 1) / 2) as usize,
        (i64::from(max).max(20) / 2) as usize,
    )
}

const BG_BACKDROP: [f32; 4] = [0.0, 0.0, 0.0, 0.5]; // Integer.MIN_VALUE (0x80000000)
const BG_ROW: [f32; 4] = [1.0, 1.0, 1.0, 0x20 as f32 / 255.0]; // 0x20FFFFFF
const COL_NAME: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const SPECTATOR_NAME_ALPHA: f32 = 0x90 as f32 / 255.0;

/// `text_width(text, FONT_SIZE)` must return the width in pixels at 1x GUI
/// scale.
pub fn build_player_tab_overlay(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    tab_list: &TabList,
    scoreboard: &Scoreboard,
    gs: f32,
    text_width: &dyn Fn(&str, f32) -> f32,
    spans_width: crate::ui::common::SpansWidthFn<'_>,
    state: &mut TabScoreState,
    gui_tick: u64,
) {
    let players = tab_list.sorted_listed(scoreboard);
    let visible_ids: HashSet<_> = players.iter().map(|p| p.uuid).collect();
    state.prune(&visible_ids);
    let hearts = scoreboard.list_objective_render_type()
        == Some(azalea_core::objectives::ObjectiveCriteria::Hearts);
    let spacer_width = text_width(" ", FONT_SIZE).ceil() as i32;
    let score_width =
        list_score_column_width(hearts, &players, scoreboard, spacer_width, |spans| {
            spans_width(spans, FONT_SIZE).ceil() as i32
        });
    if players.is_empty() {
        return;
    }

    let gw = (screen_w / gs).floor();
    let font_w = |s: &str| text_width(s, FONT_SIZE);

    let display_names: Vec<Vec<TextSpan>> = players
        .iter()
        .map(|p| display_name_for(p, scoreboard))
        .collect();
    let max_name_w: i32 = display_names
        .iter()
        .map(|n| spans_width(n, FONT_SIZE).ceil() as i32)
        .max()
        .unwrap_or(0);

    let slots = players.len();
    let mut rows = slots;
    let mut cols = 1usize;
    while rows > MAX_ROWS_PER_COL {
        cols += 1;
        rows = slots.div_ceil(cols);
    }

    let cols_i = cols as i32;
    let slot_width = player_tab_slot_width(cols_i, max_name_w, score_width, gw as i32 - 50);
    let grid_w = slot_width * cols_i + (cols_i - 1) * 5;
    let xxo = (gw as i32) / 2 - grid_w / 2;
    let cx = (gw as i32) / 2;

    let wrap_width_px = (gw - 50.0).max(1.0);
    // TODO: wrapping flattens the header/footer to plain text, dropping
    // their formatting; wrap the spans themselves to keep it.
    let header_lines = tab_list
        .header
        .as_ref()
        .map(|h| wrap_text(&span_text(h), wrap_width_px, &font_w));
    let footer_lines = tab_list
        .footer
        .as_ref()
        .map(|f| wrap_text(&span_text(f), wrap_width_px, &font_w));

    let mut max_line_w = grid_w;
    for lines in [&header_lines, &footer_lines].into_iter().flatten() {
        for l in lines {
            max_line_w = max_line_w.max(font_w(l).ceil() as i32);
        }
    }

    let push_fill =
        |elements: &mut Vec<MenuElement>, x1: i32, y1: i32, x2: i32, y2: i32, color: [f32; 4]| {
            elements.push(MenuElement::Rect {
                x: x1 as f32 * gs,
                y: y1 as f32 * gs,
                w: (x2 - x1) as f32 * gs,
                h: (y2 - y1) as f32 * gs,
                corner_radius: 0.0,
                color,
            });
        };
    let push_text =
        |elements: &mut Vec<MenuElement>, s: String, x: i32, y: i32, color: [f32; 4]| {
            elements.push(MenuElement::Text {
                x: x as f32 * gs,
                y: y as f32 * gs,
                text: s,
                scale: FONT_SIZE * gs,
                color,
                centered: false,
            });
        };
    let push_spans = |elements: &mut Vec<MenuElement>, spans: Vec<TextSpan>, x: i32, y: i32| {
        elements.push(MenuElement::TextSpans {
            x: x as f32 * gs,
            y: y as f32 * gs,
            spans,
            scale: FONT_SIZE * gs,
            centered: false,
        });
    };
    let push_image =
        |elements: &mut Vec<MenuElement>, x: i32, y: i32, w: i32, h: i32, sprite: SpriteId| {
            elements.push(MenuElement::Image {
                x: x as f32 * gs,
                y: y as f32 * gs,
                w: w as f32 * gs,
                h: h as f32 * gs,
                sprite,
                tint: [1.0, 1.0, 1.0, 1.0],
            });
        };

    let draw_text_block = |elements: &mut Vec<MenuElement>, lines: &[String], top: i32| {
        push_fill(
            elements,
            cx - max_line_w / 2 - 1,
            top - 1,
            cx + max_line_w / 2 + 1,
            top + lines.len() as i32 * LINE_HEIGHT,
            BG_BACKDROP,
        );
        for (i, line) in lines.iter().enumerate() {
            let lw = font_w(line).ceil() as i32;
            push_text(
                elements,
                line.clone(),
                cx - lw / 2,
                top + i as i32 * LINE_HEIGHT,
                COL_NAME,
            );
        }
    };

    let mut yyo: i32 = 10;
    if let Some(lines) = &header_lines {
        draw_text_block(elements, lines, yyo);
        yyo += lines.len() as i32 * LINE_HEIGHT + 1;
    }

    push_fill(
        elements,
        cx - max_line_w / 2 - 1,
        yyo - 1,
        cx + max_line_w / 2 + 1,
        yyo + rows as i32 * 9,
        BG_BACKDROP,
    );

    for i in 0..slots {
        let col = (i / rows) as i32;
        let row = (i % rows) as i32;
        let row_left = xxo + col * slot_width + col * 5;
        let yo = yyo + row * 9;

        push_fill(
            elements,
            row_left,
            yo,
            row_left + slot_width,
            yo + 8,
            BG_ROW,
        );
        let info = players[i];
        elements.push(MenuElement::SkinFace {
            x: row_left as f32 * gs,
            y: yo as f32 * gs,
            size: 8.0 * gs,
            uuid: if info.show_hat {
                info.uuid.to_string()
            } else {
                format!("{}#nohat", info.uuid)
            },
            tint: [1.0; 4],
        });
        let mut spans = display_names[i].clone();
        if info.game_mode == SPECTATOR_GAME_MODE {
            // Vanilla `decorateName` and PlayerTabOverlay's spectator color.
            for span in &mut spans {
                span.italic = true;
                span.color[3] *= SPECTATOR_NAME_ALPHA;
            }
        }
        push_spans(elements, spans, row_left + HEAD_COL_W, yo);
        if let Some(score) = visible_list_score(info, scoreboard) {
            let score_left = score_column_left(row_left, slot_width, score_width);
            if hearts {
                let health = state.update(info.uuid, score.value, gui_tick);
                draw_hearts(
                    elements,
                    score_left,
                    yo,
                    row_left + slot_width - 12,
                    score.value,
                    health,
                    gui_tick,
                    gs,
                    text_width,
                );
            } else {
                draw_integer_score_if_room(
                    elements,
                    score.formatted,
                    row_left,
                    slot_width,
                    score_width,
                    yo,
                    gs,
                    |spans| spans_width(spans, FONT_SIZE).ceil() as i32,
                );
            }
        }
        push_image(
            elements,
            row_left + slot_width - 11,
            yo,
            10,
            8,
            ping_sprite(info.latency),
        );
    }

    if let Some(lines) = &footer_lines {
        yyo += rows as i32 * 9 + 1;
        draw_text_block(elements, lines, yyo);
    }
}

fn visible_list_score<'a>(
    player: &TabListPlayer,
    scoreboard: &'a Scoreboard,
) -> Option<crate::ui::hud::ListScore> {
    (player.game_mode != SPECTATOR_GAME_MODE)
        .then(|| scoreboard.list_score(&player.name))
        .flatten()
}

fn list_score_column_width(
    hearts: bool,
    players: &[&TabListPlayer],
    scoreboard: &Scoreboard,
    spacer_width: i32,
    mut width: impl FnMut(&[TextSpan]) -> i32,
) -> i32 {
    if hearts {
        return HEART_SCORE_W;
    }
    players
        .iter()
        .filter_map(|player| visible_list_score(player, scoreboard))
        .map(|score| {
            let formatted_width = width(&score.formatted).max(0);
            if formatted_width > 0 {
                spacer_width.max(0) + formatted_width
            } else {
                0
            }
        })
        .max()
        .unwrap_or(0)
}

fn player_tab_slot_width(cols: i32, max_name_width: i32, score_width: i32, max_width: i32) -> i32 {
    ((cols * (HEAD_COL_W + max_name_width + 13 + score_width)).min(max_width) / cols).max(1)
}

fn score_column_left(row_left: i32, slot_width: i32, score_width: i32) -> i32 {
    row_left + slot_width - 12 - score_width
}

fn draw_integer_score_if_room(
    elements: &mut Vec<MenuElement>,
    spans: Vec<TextSpan>,
    row_left: i32,
    slot_width: i32,
    score_area_width: i32,
    y: i32,
    gs: f32,
    width: impl FnOnce(&[TextSpan]) -> i32,
) -> bool {
    if score_area_width <= 5 {
        return false;
    }
    draw_integer_score(elements, spans, row_left, slot_width, y, gs, width);
    true
}

fn draw_integer_score(
    elements: &mut Vec<MenuElement>,
    spans: Vec<TextSpan>,
    row_left: i32,
    slot_width: i32,
    y: i32,
    gs: f32,
    width: impl FnOnce(&[TextSpan]) -> i32,
) {
    let score_width = width(&spans).max(0);
    elements.push(MenuElement::TextSpans {
        x: score_column_left(row_left, slot_width, score_width) as f32 * gs,
        y: y as f32 * gs,
        spans,
        scale: FONT_SIZE * gs,
        centered: false,
    });
}

fn draw_hearts(
    elements: &mut Vec<MenuElement>,
    left: i32,
    y: i32,
    right: i32,
    current: i32,
    state: HealthScore,
    tick: u64,
    gs: f32,
    text_width: &dyn Fn(&str, f32) -> f32,
) {
    let (full, render) = heart_counts(current, state.displayed);
    if full == 0 {
        return;
    }
    let available = right - left;
    if available <= 5 {
        return;
    }
    let width = (((available - 4) / render.max(1) as i32).min(9)).max(0);
    if width <= 3 {
        let raw_text = float_text::java_half_heart_text(current);
        let argument = raw_text.clone();
        let hp = crate::chat_component::Component::translate(
            "multiplayer.player.list.hp",
            vec![crate::chat_component::Argument::String(argument)],
        )
        .plain_text();
        let hp_width = text_width(&hp, FONT_SIZE).ceil() as i32;
        let text = if hp_width <= available { hp } else { raw_text };
        let text_w = text_width(&text, FONT_SIZE).ceil() as i32;
        let pct = (current as f32 / 20.0).clamp(0.0, 1.0);
        let red = ((1.0 - pct) * 255.0).clamp(0.0, 255.0) as u8;
        let green = (pct * 255.0).clamp(0.0, 255.0) as u8;
        let color = [red as f32 / 255.0, green as f32 / 255.0, 0.0, 1.0];
        elements.push(MenuElement::Text {
            x: ((left + right - text_w) / 2) as f32 * gs,
            y: y as f32 * gs,
            text,
            scale: FONT_SIZE * gs,
            color,
            centered: false,
        });
        return;
    }
    let blink = heart_blink(state, tick);
    let container = if blink {
        SpriteId::TabHeartContainerBlinking
    } else {
        SpriteId::HeartContainer
    };
    let push = |elements: &mut Vec<MenuElement>, i: usize, sprite| {
        elements.push(MenuElement::Image {
            x: (left + i as i32 * width) as f32 * gs,
            y: y as f32 * gs,
            w: 9.0 * gs,
            h: 9.0 * gs,
            sprite,
            tint: [1.0; 4],
        })
    };
    for i in full..render {
        push(elements, i, container);
    }
    for i in 0..full {
        push(elements, i, container);
        if blink && state.displayed > (i as i32) * 2 {
            push(
                elements,
                i,
                if state.displayed >= (i as i32) * 2 + 2 {
                    SpriteId::TabHeartFullBlinking
                } else {
                    SpriteId::TabHeartHalfBlinking
                },
            );
        }
        if current > (i as i32) * 2 {
            push(
                elements,
                i,
                if i >= 10 {
                    if current >= (i as i32) * 2 + 2 {
                        SpriteId::TabHeartAbsorbingFullBlinking
                    } else {
                        SpriteId::TabHeartAbsorbingHalfBlinking
                    }
                } else if current >= (i as i32) * 2 + 2 {
                    SpriteId::HeartFull
                } else {
                    SpriteId::HeartHalf
                },
            );
        }
    }
}

fn display_name_for(p: &TabListPlayer, scoreboard: &Scoreboard) -> Vec<TextSpan> {
    scoreboard.player_name(&p.name, p.display_name.as_deref())
}

fn span_text(spans: &[TextSpan]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

#[cfg(test)]
mod tab_score_tests {
    use azalea_protocol::packets::game::c_set_display_objective::DisplaySlot;

    use super::*;
    use crate::player::tab_list::{PlayerInfoActions, PlayerInfoEntry};
    use crate::ui::hud::ScoreNumberFormat;

    fn list_player(name: &str, game_mode: u8) -> (TabList, uuid::Uuid) {
        let uuid = uuid::Uuid::from_u128(1);
        let mut list = TabList::new();
        list.apply_update(
            &PlayerInfoActions {
                add_player: true,
                ..Default::default()
            },
            &[PlayerInfoEntry {
                uuid,
                name: name.into(),
                textures: None,
                game_mode,
                listed: true,
                latency: 40,
                display_name: None,
                list_order: 0,
                show_hat: true,
                chat_session: None,
            }],
        );
        (list, uuid)
    }

    fn hearts_board(score: Option<i32>) -> Scoreboard {
        let mut board = Scoreboard::default();
        board.set_objective_with_render_type(
            "hp".into(),
            Some(vec![TextSpan::new("hp".into(), [1.0; 4])]),
            None,
            azalea_core::objectives::ObjectiveCriteria::Hearts,
        );
        board.set_display(DisplaySlot::List, Some("hp".into()));
        if let Some(score) = score {
            board.set_score("RawName".into(), "hp".into(), score, None, None);
        }
        board
    }

    fn render_hearts(
        current: i32,
        displayed: i32,
        tick: u64,
        left: i32,
        right: i32,
        width: impl Fn(&str, f32) -> f32,
    ) -> Vec<MenuElement> {
        let mut elements = Vec::new();
        draw_hearts(
            &mut elements,
            left,
            12,
            right,
            current,
            HealthScore {
                last: current,
                displayed,
                blink_until: tick + if displayed != current { 3 } else { 0 },
                last_update_tick: tick,
            },
            tick,
            2.0,
            &width,
        );
        elements
    }

    fn image_sprites(elements: &[MenuElement]) -> Vec<SpriteId> {
        elements
            .iter()
            .filter_map(|e| match e {
                MenuElement::Image { sprite, .. } => Some(*sprite),
                _ => None,
            })
            .collect()
    }

    fn ensure_english_hp_fixture() {
        let key = "multiplayer.player.list.hp";
        if crate::lang::translate(key).is_none() {
            let assets =
                std::env::temp_dir().join(format!("pomme-tab-lang-{}", std::process::id()));
            let lang_dir = assets.join("minecraft/lang");
            std::fs::create_dir_all(&lang_dir).unwrap();
            std::fs::copy(
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../third_party/SteelMC/steel-utils/build_assets/en_us.json"
                ),
                lang_dir.join("en_us.json"),
            )
            .unwrap();
            crate::lang::load(&assets);
        }
        let english = crate::lang::translate(key).expect("English HP fixture is loaded");
        assert_ne!(english, key);
    }

    #[test]
    fn state_change_boundaries_phases_visibility_prune_and_objective_independence() {
        let id = uuid::Uuid::nil();
        let mut state = TabScoreState::default();
        state.set_visible(true);
        assert_eq!(state.update(id, 20, 100).displayed, 20);
        let down = state.update(id, 19, 101);
        assert_eq!((down.blink_until, down.displayed), (121, 20));
        assert!(heart_blink(down, 118)); // remaining 3
        assert!(heart_blink(down, 117)); // remaining 4 is in the on phase
        assert!(!heart_blink(down, 115)); // remaining 6 is off
        assert!(!heart_blink(down, 121)); // strict deadline
        assert_eq!(state.update(id, 19, 121).displayed, 20); // elapsed exactly 20
        assert_eq!(state.update(id, 19, 122).displayed, 19); // strict >20 snap

        let up = state.update(id, 21, 130);
        assert_eq!((up.blink_until, up.displayed), (140, 19));
        assert!(!heart_blink(up, 133)); // remaining 7 => modulo 6=1
        assert!(!heart_blink(up, 134)); // remaining 6 => modulo 6=0
        assert!(heart_blink(up, 135)); // remaining 5 => on phase
        assert_eq!(state.update(id, 21, 150).displayed, 19); // exactly 20
        assert_eq!(state.update(id, 21, 151).displayed, 21); // >20

        state.set_visible(true); // same visibility does not clear
        assert!(state.health.contains_key(&id));
        state.prune(&HashSet::new());
        assert!(!state.health.contains_key(&id));
        state.update(id, 7, 200); // UUID cache has no objective key
        let cached = state.health[&id];
        let mut board = hearts_board(Some(7));
        board.set_objective_with_render_type(
            "hp".into(),
            Some(Vec::new()),
            None,
            azalea_core::objectives::ObjectiveCriteria::Integer,
        );
        assert_eq!(
            board.list_objective_render_type(),
            Some(azalea_core::objectives::ObjectiveCriteria::Integer)
        );
        assert_eq!(state.health[&id], cached);
        state.set_visible(false);
        assert!(state.health.is_empty());
        state.set_visible(false);
        assert!(state.health.is_empty());
    }

    #[test]
    fn signed_zero_missing_and_spectator_scores_remain_distinct() {
        let mut board = hearts_board(None);
        let (list, uuid) = list_player("RawName", 0);
        assert!(visible_list_score(&list.players[&uuid], &board).is_none());
        board.set_score("RawName".into(), "hp".into(), 0, None, None);
        assert_eq!(
            visible_list_score(&list.players[&uuid], &board)
                .unwrap()
                .value,
            0
        );
        board.set_score("RawName".into(), "hp".into(), -3, None, None);
        assert_eq!(
            visible_list_score(&list.players[&uuid], &board)
                .unwrap()
                .value,
            -3
        );
        let (spectator, sid) = list_player("RawName", SPECTATOR_GAME_MODE);
        assert!(visible_list_score(&spectator.players[&sid], &board).is_none());
    }

    #[test]
    fn production_tab_builder_keeps_missing_zero_and_spectator_separate() {
        let mut board = hearts_board(None);
        let (list, uuid) = list_player("RawName", 0);
        let text_width = |text: &str, _: f32| text.chars().count() as f32 * 6.0;
        let spans_width = |spans: &[TextSpan], _: f32| {
            spans
                .iter()
                .map(|span| span.text.chars().count() as f32 * 6.0)
                .sum()
        };
        let mut state = TabScoreState::default();
        state.set_visible(true);
        let mut elements = Vec::new();
        build_player_tab_overlay(
            &mut elements,
            200.0,
            &list,
            &board,
            1.0,
            &text_width,
            &spans_width,
            &mut state,
            1,
        );
        assert!(!state.health.contains_key(&uuid)); // missing LIST score is not synthetic zero

        board.set_score("RawName".into(), "hp".into(), 0, None, None);
        elements.clear();
        build_player_tab_overlay(
            &mut elements,
            200.0,
            &list,
            &board,
            1.0,
            &text_width,
            &spans_width,
            &mut state,
            1,
        );
        assert_eq!(state.health[&uuid].last, 0); // actual zero is present, though no heart fill paints
        assert!(image_sprites(&elements).iter().all(|id| !matches!(
            id,
            SpriteId::HeartContainer | SpriteId::HeartFull | SpriteId::HeartHalf
        )));

        let (spectators, spectator_id) = list_player("RawName", SPECTATOR_GAME_MODE);
        board.set_score("RawName".into(), "hp".into(), 12, None, None);
        let mut spectator_state = TabScoreState::default();
        spectator_state.set_visible(true);
        elements.clear();
        build_player_tab_overlay(
            &mut elements,
            200.0,
            &spectators,
            &board,
            1.0,
            &text_width,
            &spans_width,
            &mut spectator_state,
            2,
        );
        assert!(!spectator_state.health.contains_key(&spectator_id));
        assert!(image_sprites(&elements).iter().all(|id| !matches!(
            id,
            SpriteId::HeartContainer | SpriteId::HeartFull | SpriteId::HeartHalf
        )));
    }

    #[test]
    fn count_layout_and_icon_sequence_cover_odd_absorption_and_old_overlay() {
        assert_eq!(heart_counts(1, 1), (1, 10));
        assert_eq!(heart_counts(21, 21), (11, 10));
        assert_eq!(heart_counts(i32::MAX, i32::MAX).0, 1_073_741_824);
        assert_eq!(heart_counts(i32::MIN, i32::MIN), (0, 10));
        let width = |s: &str, _: f32| s.chars().count() as f32 * 6.0;
        let odd = render_hearts(1, 1, 0, 0, 90, width);
        let odd_sprites = image_sprites(&odd);
        assert_eq!(odd_sprites.len(), 11); // nine empty containers, then filled cell
        assert_eq!(&odd_sprites[..9], &[SpriteId::HeartContainer; 9]);
        assert_eq!(odd_sprites[9], SpriteId::HeartContainer);
        assert_eq!(odd_sprites[10], SpriteId::HeartHalf);
        assert!(odd.iter().all(|element| !matches!(element, MenuElement::Image { w, h, .. } if *w != 18.0 || *h != 18.0)));

        let layered = render_hearts(13, 16, 5, 0, 90, width);
        let sprites = image_sprites(&layered);
        assert_eq!(heart_counts(13, 16), (8, 10)); // two trailing containers
        assert_eq!(&sprites[..3], &[SpriteId::TabHeartContainerBlinking; 3]); // trailing + first filled cell
        assert_eq!(sprites[3], SpriteId::TabHeartFullBlinking);
        assert_eq!(sprites[4], SpriteId::HeartFull); // old overlay precedes current fill
        // Heart index 10 is current half absorption, always the absorbing sprite.
        let absorption_half = render_hearts(21, 22, 5, 0, 90, width);
        assert!(image_sprites(&absorption_half).contains(&SpriteId::TabHeartAbsorbingHalfBlinking));
        let absorption_full = render_hearts(22, 23, 5, 0, 90, width);
        assert!(image_sprites(&absorption_full).contains(&SpriteId::TabHeartAbsorbingFullBlinking));
        assert_eq!(sprites.len(), 25); // 10 containers + 8 old overlays + 7 current fills
    }

    #[test]
    fn narrow_text_translation_fallback_rgb_center_and_huge_scores_are_bounded() {
        ensure_english_hp_fixture();
        let translated = render_hearts(1, 1, 0, 10, 50, |s, _| s.chars().count() as f32 * 4.0);
        let MenuElement::Text {
            x, y, text, color, ..
        } = &translated[0]
        else {
            panic!("expected translated text")
        };
        assert!(text.ends_with("hp"));
        // English `%shp` has no space: `1.0hp` is five characters at 4px each.
        assert_eq!((*x, *y), (40.0, 24.0));
        assert_eq!(*color, [242.0 / 255.0, 12.0 / 255.0, 0.0, 1.0]);

        let raw = render_hearts(10, 10, 0, 10, 30, |s, _| {
            if s.ends_with("hp") { 30.0 } else { 12.0 }
        });
        let MenuElement::Text { text, color, x, .. } = &raw[0] else {
            panic!("expected raw float fallback")
        };
        assert_eq!(text, &float_text::java_half_heart_text(10));
        assert_eq!(*color, [127.0 / 255.0, 127.0 / 255.0, 0.0, 1.0]);
        assert_eq!(*x, 28.0);

        for score in [i32::MIN, i32::MAX] {
            let bounded = render_hearts(score, score, 0, 0, 90, |s, _| s.len() as f32);
            assert!(
                bounded.len() <= 1,
                "large count must not become an icon loop"
            );
        }
        assert!(render_hearts(1, 1, 0, 0, 5, width_unknown).is_empty());
    }

    #[test]
    fn formatted_blank_and_missing_columns_measure_spacer_and_right_align_actual_spans() {
        let mut board = Scoreboard::default();
        board.set_objective("n".into(), Some(Vec::new()), None);
        board.set_display(DisplaySlot::List, Some("n".into()));
        board.set_score(
            "RawName".into(),
            "n".into(),
            4,
            None,
            Some(ScoreNumberFormat::Fixed(vec![TextSpan::new(
                "1234".into(),
                [1.0; 4],
            )])),
        );
        let (mut list, _) = list_player("RawName", 0);
        let other_id = uuid::Uuid::from_u128(2);
        list.apply_update(
            &PlayerInfoActions {
                add_player: true,
                ..Default::default()
            },
            &[PlayerInfoEntry {
                uuid: other_id,
                name: "Other".into(),
                textures: None,
                game_mode: 0,
                listed: true,
                latency: 40,
                display_name: None,
                list_order: 0,
                show_hat: true,
                chat_session: None,
            }],
        );
        board.set_score(
            "Other".into(),
            "n".into(),
            2,
            None,
            Some(ScoreNumberFormat::Fixed(vec![TextSpan::new(
                "8".into(),
                [1.0; 4],
            )])),
        );
        let players = list.sorted_listed(&board);
        let raw_player = players
            .iter()
            .find(|player| player.name == "RawName")
            .unwrap();
        let measure =
            |spans: &[TextSpan]| spans.iter().map(|span| span.text.len() as i32 * 6).sum();
        let reserved = list_score_column_width(false, &players, &board, 4, measure);
        assert_eq!(reserved, 28); // max(24 + measured space 4, 6 + 4)
        assert_eq!(player_tab_slot_width(1, 30, reserved, 1_000), 80);
        assert_eq!(player_tab_slot_width(2, 30, reserved, 200), 80);
        assert_eq!(player_tab_slot_width(1, 30, reserved, 50), 50);
        assert_eq!(player_tab_slot_width(1, 30, 0, 1_000), 52);
        assert_eq!(player_tab_slot_width(1, 30, HEART_SCORE_W, 1_000), 142);

        board.set_score(
            "RawName".into(),
            "n".into(),
            4,
            None,
            Some(ScoreNumberFormat::Blank),
        );
        assert_eq!(
            list_score_column_width(false, &[*raw_player], &board, 4, measure),
            0
        );
        board.set_score(
            "Other".into(),
            "n".into(),
            2,
            None,
            Some(ScoreNumberFormat::Blank),
        );
        assert_eq!(
            list_score_column_width(false, &players, &board, 4, measure),
            0
        );
        board.reset_score("RawName", Some("n"));
        assert_eq!(
            list_score_column_width(false, &[players[0]], &board, 4, measure),
            0
        );
        assert_eq!(
            list_score_column_width(true, &players, &board, 4, |_| unreachable!()),
            HEART_SCORE_W
        );

        let mut rendered = Vec::new();
        let mut rich = TextSpan::new("1234".into(), [1.0; 4]);
        rich.bold = true;
        assert!(draw_integer_score_if_room(
            &mut rendered,
            vec![rich],
            0,
            80,
            reserved,
            7,
            1.0,
            measure
        ));
        assert!(
            matches!(&rendered[0], MenuElement::TextSpans { x: 44.0, spans, .. } if spans[0].text == "1234" && spans[0].bold)
        );
        assert!(draw_integer_score_if_room(
            &mut rendered,
            vec![TextSpan::new("8".into(), [1.0; 4])],
            0,
            80,
            reserved,
            7,
            1.0,
            measure,
        ));
        assert!(
            matches!(&rendered[1], MenuElement::TextSpans { x: 62.0, spans, .. } if spans[0].text == "8")
        );
        assert!(draw_integer_score_if_room(
            &mut rendered,
            Vec::new(),
            0,
            80,
            reserved,
            7,
            1.0,
            |_| 0
        ));
        assert!(
            matches!(&rendered[2], MenuElement::TextSpans { x: 68.0, spans, .. } if spans.is_empty())
        );
        assert!(!draw_integer_score_if_room(
            &mut rendered,
            Vec::new(),
            0,
            80,
            5,
            7,
            1.0,
            |_| 0
        ));
        assert_eq!(rendered.len(), 3); // official score-area gate is >5
        assert_eq!(score_column_left(0, 80, 24), 44); // reserved spacer is not text width
        assert_eq!(list_score_column_width(false, &[], &board, 4, |_| 0), 0);
    }

    fn width_unknown(_: &str, _: f32) -> f32 {
        8.0
    }
}

pub struct PlayerNameplates<'a> {
    pub entity_store: &'a EntityStore,
    pub tab_list: &'a TabList,
    pub scoreboard: &'a Scoreboard,
    pub local_uuid: uuid::Uuid,
    pub partial_tick: f32,
    pub gs: f32,
    pub camera_pos: glam::DVec3,
    pub project: &'a dyn Fn(glam::DVec3) -> Option<(f32, f32)>,
}

// TODO: vanilla renders name tags as world-space billboards (0.025 scale,
// shrinking with distance, 25% black backdrop, a see-through pass behind
// walls, sneak dimming, no shadow) and honors team nametagVisibility; this
// screen-space projection is an approximation until a world-space text path
// exists.
pub fn build_player_nameplates(elements: &mut Vec<MenuElement>, nameplates: PlayerNameplates<'_>) {
    for entity in nameplates.entity_store.living.values() {
        let Some(uuid) = entity.player_uuid else {
            continue;
        };
        if uuid == nameplates.local_uuid {
            continue;
        }
        let Some(player) = nameplates.tab_list.players.get(&uuid) else {
            continue;
        };
        let pos = entity
            .prev_position
            .lerp(entity.position, nameplates.partial_tick as f64)
            // Vanilla anchors at the pose bounding-box height + 0.5.
            + glam::DVec3::Y * if entity.is_crouching { 2.0 } else { 2.3 };
        let max_distance = if entity.is_crouching { 32.0 } else { 64.0 };
        if (*pos - nameplates.camera_pos).length_squared() > max_distance * max_distance {
            continue;
        }
        let Some((x, y)) = (nameplates.project)(*pos) else {
            continue;
        };
        elements.push(MenuElement::TextSpans {
            x,
            y: y - 4.0 * nameplates.gs,
            // The tab-list display name is tab-only in vanilla; name tags
            // always use the team-formatted profile name.
            spans: nameplates.scoreboard.player_name(&player.name, None),
            scale: FONT_SIZE * nameplates.gs,
            centered: true,
        });
    }
}

fn ping_sprite(latency: i32) -> SpriteId {
    if latency < 0 {
        SpriteId::PingUnknown
    } else if latency < 150 {
        SpriteId::Ping5
    } else if latency < 300 {
        SpriteId::Ping4
    } else if latency < 600 {
        SpriteId::Ping3
    } else if latency < 1000 {
        SpriteId::Ping2
    } else {
        SpriteId::Ping1
    }
}

fn wrap_text(text: &str, max_width: f32, font_w: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for raw_line in text.split('\n') {
        let mut current = String::new();
        for word in raw_line.split(' ') {
            if current.is_empty() {
                current.push_str(word);
                continue;
            }
            if font_w(&format!("{current} {word}")) <= max_width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        lines.push(current);
    }
    lines
}
