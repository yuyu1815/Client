use super::*;
use crate::resource_pack::PackCompat;

// Keep the English row strings as control IDs. The grid resolves sliders,
// disabled controls, navigation and click handlers against those IDs before
// translating only the text passed to the renderer.
fn option_text(label: &str) -> String {
    option_text_for(crate::lang::locale(), label)
}

fn option_text_for(locale: &str, label: &str) -> String {
    let (name, value) = label
        .split_once(": ")
        .map_or((label, None), |(n, v)| (n, Some(v)));
    let (key, ja) = match name {
        "Options" => ("options.title", "設定"),
        "Language" => ("options.language.title", "言語"),
        "Online..." => ("options.online", "オンライン..."),
        "Online Options..." => ("options.online.title", "オンライン設定..."),
        "Skin Customization..." => ("options.skinCustomisation", "スキンのカスタマイズ..."),
        "Skin Customization" => ("options.skinCustomisation.title", "スキンのカスタマイズ"),
        "Music & Sounds..." => ("options.sounds", "音楽とサウンド..."),
        "Music & Sounds" => ("options.sounds.title", "音楽とサウンド"),
        "Video Settings..." => ("options.video", "ビデオ設定..."),
        "Video Settings" => ("options.videoTitle", "ビデオ設定"),
        "Controls..." => ("options.controls", "操作設定..."),
        "Controls" => ("", "操作設定"),
        "Chat Settings..." => ("options.chat", "チャット設定..."),
        "Chat Settings" => ("options.chat.title", "チャット設定"),
        "Resource Packs..." => ("options.resourcepack", "リソースパック..."),
        "Accessibility Settings..." => ("options.accessibility", "アクセシビリティ設定..."),
        "Accessibility Settings" => ("options.accessibility.title", "アクセシビリティ設定"),
        "Telemetry Data..." => ("options.telemetry", "テレメトリーデータ..."),
        "Credits & Attribution..." => {
            ("options.credits_and_attribution", "クレジットと帰属表示...")
        }
        "Display" => ("options.video.display.header", "ディスプレイ"),
        "Quality" => ("", "画質"),
        "Preferences" => ("options.video.preferences.header", "環境設定"),
        "FOV" => ("options.fov", "視野角"),
        "Fullscreen Resolution" => ("options.fullscreen.resolution", "フルスクリーン解像度"),
        "Max Framerate" => ("options.framerateLimit", "最大フレームレート"),
        "VSync" => ("", "垂直同期"),
        "Inactivity FPS Limit" => ("options.inactivityFpsLimit", "非アクティブ時のFPS制限"),
        "GUI Scale" => ("options.guiScale", "GUIの大きさ"),
        "Fullscreen" => ("options.fullscreen", "フルスクリーン"),
        "Exclusive Fullscreen" => ("options.exclusiveFullscreen", "排他的フルスクリーン"),
        "Brightness" => ("options.gamma", "明るさ"),
        "Graphics Backend" => ("", "描画方式"),
        "Graphics" => ("options.graphics", "グラフィックス"),
        "Biome Blend" => ("options.biomeBlendRadius", "バイオームの混合"),
        "Render Distance" => ("options.renderDistance", "描画距離"),
        "Prioritize Chunk Updates" => ("", "チャンク更新の優先度"),
        "Simulation Distance" => ("options.simulationDistance", "シミュレーション距離"),
        "Smooth Lighting" => ("options.ao", "スムースライティング"),
        "Clouds" => ("options.renderClouds", "雲"),
        "Chunk Detail" => ("", "チャンクの詳細度"),
        "Particles" => ("options.particles", "パーティクル"),
        "Mipmap Levels" => ("options.mipmapLevels", "ミップマップレベル"),
        "Entity Shadows" => ("options.entityShadows", "エンティティの影"),
        "Entity Distance" => ("options.entityDistanceScaling", "エンティティの描画距離"),
        "Menu Background Blur" => (
            "options.accessibility.menu_background_blurriness",
            "メニュー背景のぼかし",
        ),
        "Cloud Range" => ("", "雲の表示範囲"),
        "Cutout Leaves" => ("options.cutoutLeaves", "葉の透過"),
        "Improved Transparency" => ("options.improvedTransparency", "透過表現の改善"),
        "Texture Filtering" => ("", "テクスチャフィルタリング"),
        "Max Anisotropy" => ("options.maxAnisotropy", "異方性フィルタリング"),
        "Weather Radius" => ("", "天候の表示範囲"),
        "Show Autosave Indicator" => ("options.autosaveIndicator", "自動保存表示"),
        "Vignette" => ("options.vignette", "周辺減光"),
        "Attack Indicator" => ("options.attackIndicator", "攻撃インジケーター"),
        "Chunk Fade-in" => ("", "チャンクのフェードイン"),
        "Sensitivity" => ("options.sensitivity", "マウス感度"),
        "Invert Mouse" => ("options.invertMouse", "マウスの反転"),
        "Auto-Jump" => ("options.autoJump", "自動ジャンプ"),
        "Operator Items Tab" => ("", "オペレーターアイテムタブ"),
        "Key Binds..." => ("", "キー設定..."),
        "Mouse Settings..." => ("options.mouse_settings", "マウス設定..."),
        "Sneak" => ("", "スニーク"),
        "Sprint" => ("", "ダッシュ"),
        "Chat" => ("options.chat.visibility", "チャット"),
        "Colors" => ("options.chat.color", "色"),
        "Web Links" => ("options.chat.links", "ウェブリンク"),
        "Prompt on Links" => ("", "リンクを開く前に確認"),
        "Chat Text Opacity" => ("options.chat.opacity", "チャット文字の不透明度"),
        "Text Background Opacity" => (
            "options.accessibility.text_background_opacity",
            "文字の背景の不透明度",
        ),
        "Chat Text Size" => ("options.chat.scale", "チャット文字の大きさ"),
        "Line Spacing" => ("options.chat.line_spacing", "行間"),
        "Chat Delay" => ("", "チャットの遅延"),
        "Width" => ("options.chat.width", "幅"),
        "Focused Height" => ("options.chat.height.focused", "表示中の高さ"),
        "Unfocused Height" => ("options.chat.height.unfocused", "非表示中の高さ"),
        "Narrator" => ("options.narrator", "ナレーター"),
        "Command Suggestions" => ("options.autoSuggestCommands", "コマンド候補"),
        "Hide Matched Names" => ("options.hideMatchedNames", "一致する名前を非表示"),
        "Reduced Debug Info" => ("", "デバッグ情報を減らす"),
        "Only Show Secure Chat" => ("options.onlyShowSecureChat", "安全なチャットのみ表示"),
        "Save Unsent Chats" => ("options.chat.drafts", "未送信のチャットを保存"),
        "Show Subtitles" => ("options.showSubtitles", "字幕を表示"),
        "High Contrast" => ("options.accessibility.high_contrast", "ハイコントラスト"),
        "Background for Chat Only" => ("", "チャットのみ背景を表示"),
        "Notification Time" => ("", "通知の表示時間"),
        "View Bobbing" => ("options.viewBobbing", "視点の揺れ"),
        "Distortion Effects" => ("", "画面の歪み"),
        "FOV Effects" => ("options.fovEffectScale", "視野角への効果"),
        "Darkness Pulsing" => ("options.darknessEffectScale", "暗闇の脈動"),
        "Damage Tilt" => ("options.damageTiltStrength", "ダメージ時の傾き"),
        "Glint Speed" => ("options.glintSpeed", "エンチャントの輝きの速度"),
        "Glint Strength" => ("options.glintStrength", "エンチャントの輝きの強さ"),
        "Hide Lightning Flashes" => ("options.hideLightningFlashes", "稲光を非表示"),
        "Dark Loading Screen" => ("", "暗いロード画面"),
        "Panorama Scroll Speed" => (
            "options.accessibility.panorama_speed",
            "パノラマのスクロール速度",
        ),
        "Hide Splash Texts" => ("options.hideSplashTexts", "スプラッシュを非表示"),
        "Narrator Hotkey" => (
            "options.accessibility.narrator_hotkey",
            "ナレーターのショートカット",
        ),
        "Rotate with Minecart" => ("", "トロッコに合わせて回転"),
        "High Contrast Outlines" => (
            "options.accessibility.high_contrast_block_outline",
            "ハイコントラストな輪郭",
        ),
        "Master Volume" => ("", "全体の音量"),
        "Music" => ("", "音楽"),
        "Jukebox/Note Blocks" => ("", "ジュークボックス/音符ブロック"),
        "Weather" => ("", "天候"),
        "Blocks" => ("", "ブロック"),
        "Hostile Creatures" => ("", "敵対的な生物"),
        "Friendly Creatures" => ("", "友好的な生物"),
        "Players" => ("", "プレイヤー"),
        "Ambient/Environment" => ("", "環境音"),
        "Voice/Speech" => ("", "声/会話"),
        "UI" => ("", "UI"),
        "Device" => ("options.audioDevice", "音声デバイス"),
        "Directional Audio" => ("options.directionalAudio", "立体音響"),
        "Music Frequency" => ("options.music_frequency", "音楽の頻度"),
        "Music Toast" => ("options.musicToast", "音楽の通知"),
        "Cape" => ("options.modelPart.cape", "マント"),
        "Jacket" => ("options.modelPart.jacket", "上着"),
        "Left Sleeve" => ("options.modelPart.left_sleeve", "左袖"),
        "Right Sleeve" => ("options.modelPart.right_sleeve", "右袖"),
        "Left Pants Leg" => ("options.modelPart.left_pants_leg", "左のズボン"),
        "Right Pants Leg" => ("options.modelPart.right_pants_leg", "右のズボン"),
        "Hat" => ("options.modelPart.hat", "帽子"),
        "Main Hand" => ("options.mainHand", "利き手"),
        "Realms Notifications" => ("", "Realmsの通知"),
        "Allow Server Listings" => ("options.allowServerListing", "サーバーの一覧表示を許可"),
        "Show Online Status" => ("", "オンライン状態を表示"),
        "Show Current Server" => ("", "現在のサーバーを表示"),
        _ => return label.to_string(),
    };
    let translated = if locale == "ja_jp" {
        if key.is_empty() {
            ja
        } else {
            crate::lang::translate_for(locale, key).unwrap_or(ja)
        }
    } else {
        name
    };
    match value {
        Some(value) => format!("{translated}: {}", option_value_for(locale, value)),
        None => translated.to_owned(),
    }
}

fn option_value_for(locale: &str, value: &str) -> String {
    if locale != "ja_jp" {
        return value.to_owned();
    }
    let (key, ja) = match value {
        "ON" => ("options.on", "オン"),
        "OFF" => ("options.off", "オフ"),
        "Auto" => ("options.guiScale.auto", "自動"),
        "Normal" => ("options.fov.min", "標準"),
        "Quake Pro" => ("options.fov.max", "最大"),
        "Unlimited" => ("options.framerateLimit.max", "無制限"),
        "Fancy" => ("options.graphics.fancy", "高品質"),
        "All" => ("options.particles.all", "すべて"),
        "Decreased" => ("options.particles.decreased", "減少"),
        "Minimal" => ("options.particles.minimal", "最小限"),
        "Default" => ("options.graphicsApi.default", "デフォルト"),
        "Current" => ("options.fullscreen.current", "現在の設定"),
        "Right" => ("options.mainHand.right", "右"),
        "Left" => ("options.mainHand.left", "左"),
        "Toggle" => ("options.key.toggle", "切り替え"),
        "Hold" => ("options.key.hold", "長押し"),
        "*yawn*" => ("options.sensitivity.min", "とても遅い"),
        "HYPERSPEED!!!" => ("options.sensitivity.max", "超高速!!!"),
        "None" => ("", "なし"),
        "Windowed" => ("", "ウィンドウ"),
        "Borderless" => ("", "枠なし"),
        "Exclusive" => ("", "排他モード"),
        "Off" => ("options.off", "オフ"),
        "Fast" => ("options.clouds.fast", "高速"),
        "Crosshair" => ("options.attack.crosshair", "クロスヘア"),
        "Hotbar" => ("options.attack.hotbar", "ホットバー"),
        "Shown" => ("options.chat.visibility.full", "表示"),
        "Commands Only" => ("options.chat.visibility.system", "コマンドのみ"),
        "Hidden" => ("options.chat.visibility.hidden", "非表示"),
        "1 minute" => ("", "1分"),
        "1.0s" => ("", "1.0秒"),
        "10.0s" => ("", "10.0秒"),
        _ => ("", ""),
    };
    if !ja.is_empty() {
        return if key.is_empty() {
            ja.to_owned()
        } else {
            localized_option_value(value, ja, crate::lang::translate_for(locale, key))
        };
    }
    if let Some(n) = value.strip_suffix(" chunks") {
        return format!("{n}{}", crate::lang::ui_for(locale, " chunks", "チャンク"));
    }
    if let Some(n) = value.strip_suffix(" second(s)") {
        return format!("{n}{}", crate::lang::ui_for(locale, " second(s)", "秒"));
    }
    value.to_owned()
}

// `translate` falls back to the English catalog when the Japanese asset is
// missing. Keep common toggle values Japanese in that case.
fn localized_option_value(value: &str, ja: &str, translated: Option<&str>) -> String {
    if matches!(value, "ON" | "OFF" | "Off")
        && translated.is_some_and(|text| text.eq_ignore_ascii_case(value))
    {
        ja.to_owned()
    } else {
        translated.unwrap_or(ja).to_owned()
    }
}

/// A row in a vanilla-style options list: 25px pitch, a 310px `Big` widget, or
/// two 150px widgets per `Pair` (`PairLeft` for an odd trailing widget).
pub(super) enum OptRow<'a> {
    Header(&'a str),
    Big(&'a str),
    Pair(&'a str, &'a str),
    PairLeft(&'a str),
}

fn option_enabled(label: &str, disabled: &[&str]) -> bool {
    !disabled.iter().any(|prefix| label.starts_with(prefix))
}

/// Widget labels a row builds; `Header` rows are text, not widgets. Only the
/// disable-list guard needs this, so it is debug-only.
#[cfg(debug_assertions)]
fn row_labels<'a>(row: &OptRow<'a>) -> Vec<&'a str> {
    match row {
        OptRow::Header(_) => Vec::new(),
        OptRow::Big(a) | OptRow::PairLeft(a) => vec![*a],
        OptRow::Pair(a, b) => vec![*a, *b],
    }
}

fn compat_label(compat: PackCompat) -> (&'static str, [f32; 4]) {
    match compat {
        PackCompat::Compatible => (
            crate::lang::ui("Compatible", "互換性あり"),
            [0.33, 0.87, 0.33, 1.0],
        ),
        PackCompat::TooOld => (
            crate::lang::ui("Made for an older version", "古いバージョン向け"),
            COL_RED,
        ),
        PackCompat::TooNew => (
            crate::lang::ui("Made for a newer version", "新しいバージョン向け"),
            COL_RED,
        ),
    }
}

fn option_tooltip(tip: &str) -> String {
    let (key, ja) = match tip {
        "3rd-party Servers may send chat messages in non-standard formats.\nWith this option on, hidden players will be matched based on chat sender names." => {
            (
                "options.hideMatchedNames.tooltip",
                "外部サーバーでは標準以外の形式のチャットが送信される場合があります。\n有効にすると、非表示のプレイヤーを送信者名から判定します。",
            )
        }
        "Only display messages from other players that can be verified to have been sent by that player, and have not been modified." => {
            (
                "options.onlyShowSecureChat.tooltip",
                "送信者を確認でき、改変されていないメッセージのみ表示します。",
            )
        }
        "Unsent messages will be saved and can be sent the next time chat is opened." => (
            "",
            "未送信のメッセージを保存し、次にチャットを開いたときに送信できます。",
        ),
        "Receive notifications about Realms updates" => ("", "Realmsの更新通知を受け取ります。"),
        "Allow servers to list your name in their player list" => (
            "",
            "サーバーのプレイヤー一覧に名前を表示することを許可します。",
        ),
        "Allow friends to see when you're online" => ("", "フレンドにオンライン状態を公開します。"),
        "Allow friends to see which server you're on" => {
            ("", "フレンドに現在のサーバーを公開します。")
        }
        _ => return tip.to_owned(),
    };
    if crate::lang::locale() == "ja_jp" {
        if key.is_empty() {
            ja
        } else {
            crate::lang::translate(key).unwrap_or(ja)
        }
        .to_owned()
    } else {
        tip.to_owned()
    }
}

// Chat option label prefixes, shared by the labels and their handlers.
const ENTITY_DISTANCE: &str = "Entity Distance:";

fn entity_distance_prefix() -> String {
    format!(
        "{}:",
        crate::lang::translate("options.entityDistanceScaling").unwrap_or("Entity Distance")
    )
}

fn slider_matches(label: &str, id: &str, entity_prefix: &str) -> bool {
    label.starts_with(id) || (id == ENTITY_DISTANCE && label.starts_with(entity_prefix))
}

const CHAT_VISIBILITY: &str = "Chat:";
const CHAT_COLORS: &str = "Colors:";
const CHAT_LINKS: &str = "Web Links:";
const CHAT_LINKS_PROMPT: &str = "Prompt on Links:";
const CHAT_OPACITY: &str = "Chat Text Opacity:";
const TEXT_BACKGROUND_OPACITY: &str = "Text Background Opacity:";
const CHAT_SCALE: &str = "Chat Text Size:";
const CHAT_LINE_SPACING: &str = "Line Spacing:";
const CHAT_DELAY: &str = "Chat Delay:";
const CHAT_WIDTH: &str = "Width:";
const CHAT_HEIGHT_FOCUSED: &str = "Focused Height:";
const CHAT_HEIGHT_UNFOCUSED: &str = "Unfocused Height:";
const AUTO_SUGGESTIONS: &str = "Command Suggestions:";
const HIDE_MATCHED_NAMES: &str = "Hide Matched Names:";
const ONLY_SHOW_SECURE_CHAT: &str = "Only Show Secure Chat:";
const SAVE_CHAT_DRAFTS: &str = "Save Unsent Chats:";

/// The `chatDelay` slider's `IntRange(0, 60)`, in tenths of a second.
const CHAT_DELAY_MAX_TENTHS: i32 = 60;

/// Vanilla `Options.percentValueLabel`, which truncates.
fn percent_label(prefix: &str, value: f64) -> String {
    format!("{prefix} {}%", (value * 100.0) as i32)
}

fn chat_opacity_label(opacity: f32) -> String {
    percent_label(CHAT_OPACITY, f64::from(opacity) * 0.9 + 0.1)
}

fn chat_scale_label(scale: f32) -> String {
    if scale == 0.0 {
        format!("{CHAT_SCALE} OFF")
    } else {
        percent_label(CHAT_SCALE, scale.into())
    }
}

fn chat_delay_label(secs: f32) -> String {
    if secs <= 0.0 {
        format!("{CHAT_DELAY} None")
    } else {
        format!("{CHAT_DELAY} {secs:.1} second(s)")
    }
}

/// `IntRangeBase.toSliderValue` on the delay's tenths.
fn chat_delay_slider(secs: f32) -> f32 {
    match (secs * 10.0) as i32 {
        0 => 0.0,
        CHAT_DELAY_MAX_TENTHS => 1.0,
        n => (n as f32 + 0.5) / (CHAT_DELAY_MAX_TENTHS + 1) as f32,
    }
}

/// `IntRangeBase.fromSliderValue`, back to seconds.
fn chat_delay_from_slider(slider: f32) -> f32 {
    let slider = if slider >= 1.0 { 0.99999 } else { slider };
    (slider * (CHAT_DELAY_MAX_TENTHS + 1) as f32).floor() / 10.0
}

impl MainMenu {
    pub(super) fn build_options(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        // Sub-screens reached from here (Language/Accessibility/Chat) return
        // to Options.
        self.settings_back = Screen::Options;
        let fov_label = if self.fov == 70 {
            "FOV: Normal".to_string()
        } else if self.fov >= 110 {
            "FOV: Quake Pro".to_string()
        } else {
            format!("FOV: {}", self.fov)
        };
        // FOV slider + Online lead the grid, above the categories (vanilla header
        // sub-row).
        let language_label = crate::lang::translate("options.language").unwrap_or("Language...");
        let rows: Vec<OptRow> = vec![
            OptRow::Pair(&fov_label, "Online..."),
            OptRow::Pair("Skin Customization...", "Music & Sounds..."),
            OptRow::Pair("Video Settings...", "Controls..."),
            OptRow::Pair(language_label, "Chat Settings..."),
            OptRow::Pair("Resource Packs...", "Accessibility Settings..."),
            OptRow::Pair("Telemetry Data...", "Credits & Attribution..."),
        ];

        let nav: &[(&str, Screen)] = &[
            ("Online...", Screen::OptionsOnline),
            ("Skin Customization...", Screen::OptionsSkinCustomization),
            ("Music & Sounds...", Screen::OptionsMusicSounds),
            ("Video Settings...", Screen::OptionsVideo),
            ("Controls...", Screen::OptionsControls),
            (language_label, Screen::OptionsLanguage),
            ("Chat Settings...", Screen::OptionsChatSettings),
            ("Resource Packs...", Screen::OptionsResourcePacks),
            ("Accessibility Settings...", Screen::OptionsAccessibility),
            ("Telemetry Data...", Screen::OptionsTelemetry),
            ("Credits & Attribution...", Screen::OptionsCredits),
        ];

        let fov_frac = (self.fov as f32 - 30.0) / 80.0;
        let sliders: &[(&str, f32)] = &[("FOV:", fov_frac)];
        // Nav rows are disabled only where the target is a `build_options_stub`
        // page; screens with real (if inert) controls stay reachable.
        let disabled = &["Telemetry Data..."];
        self.build_options_grid(
            sw,
            sh,
            input,
            crate::lang::translate("options.title").unwrap_or("Options"),
            Screen::Main,
            &rows,
            nav,
            sliders,
            disabled,
            false,
            &[],
            text_width_fn,
        )
    }

    pub(super) fn build_options_language(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        let english = if self.locale == "en_us" {
            "English (US) ✓"
        } else {
            "English (US)"
        };
        let japanese = if self.locale == "ja_jp" {
            "日本語 (日本) ✓"
        } else {
            "日本語 (日本)"
        };
        self.build_options_grid(
            sw,
            sh,
            input,
            crate::lang::translate("options.language.title").unwrap_or("Language"),
            self.settings_back.clone_screen(),
            &[OptRow::Pair(english, japanese)],
            &[],
            &[],
            &[],
            true,
            &[],
            text_width_fn,
        )
    }

    fn view_bobbing_label(&self) -> &'static str {
        if self.view_bobbing {
            "View Bobbing: ON"
        } else {
            "View Bobbing: OFF"
        }
    }

    fn show_subtitles_label(&self) -> &'static str {
        if self.show_subtitles {
            "Show Subtitles: ON"
        } else {
            "Show Subtitles: OFF"
        }
    }

    /// Value count minus one for the discrete sliders, which Left/Right step
    /// one value at a time; `None` for the unit sliders, which nudge by
    /// `1 / (width - 8)` (`OptionInstanceSliderButton.keyPressed`).
    fn discrete_slider_span(&self, prefix: &str) -> Option<f32> {
        Some(match prefix {
            "Render Distance:" => self.render_distance_max() as f32 - 2.0,
            ENTITY_DISTANCE => 18.0,
            "Chunk Detail:" => 40.0,
            "Simulation Distance:" => 27.0,
            "Max Framerate:" => 25.0,
            "FOV:" => 80.0,
            CHAT_DELAY => CHAT_DELAY_MAX_TENTHS as f32,
            _ => return None,
        })
    }

    fn slider_key_step(&self, prefix: &str, w: f32, gs: f32) -> f32 {
        self.discrete_slider_span(prefix)
            .map_or(gs / (w - 8.0 * gs), |span| 1.0 / span)
    }

    /// Upper bound of the render distance slider: the server-announced view
    /// distance while connected (can exceed 32), 32 otherwise.
    fn render_distance_max(&self) -> u32 {
        if self.server_render_distance > 0 {
            self.server_render_distance.max(3)
        } else {
            32
        }
    }

    pub(super) fn build_options_video(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        let fullscreen_label = match self.display_mode {
            DisplayMode::Windowed => "Fullscreen: Windowed",
            DisplayMode::Borderless => "Fullscreen: Borderless",
            DisplayMode::Fullscreen => "Fullscreen: Exclusive",
        };
        let rd_max = self.render_distance_max();
        let rd = format!(
            "Render Distance: {} chunks",
            self.render_distance.min(rd_max)
        );
        let cd = format!("Chunk Detail: {} chunks", self.chunk_detail);
        let ed = format!(
            "{} {}%",
            entity_distance_prefix(),
            self.entity_distance_percent
        );
        let sd = format!("Simulation Distance: {} chunks", self.simulation_distance);
        let mf = if self.max_framerate >= super::MAX_FRAMERATE_UNLIMITED {
            "Max Framerate: Unlimited".to_string()
        } else {
            format!("Max Framerate: {} fps", self.max_framerate)
        };
        let gui_label = if self.gui_scale_setting == 0 {
            "GUI Scale: Auto".to_string()
        } else {
            format!("GUI Scale: {}", self.gui_scale_setting)
        };
        let vsync_label = if self.vsync {
            "VSync: ON"
        } else {
            "VSync: OFF"
        };
        let clouds_label = format!("Clouds: {}", self.cloud_mode.label());
        let attack_label = format!("Attack Indicator: {}", self.attack_indicator.label());
        let particles_label = format!(
            "Particles: {}",
            match self.particle_status() {
                crate::particle::ParticleMode::All => "All",
                crate::particle::ParticleMode::Decreased => "Decreased",
                crate::particle::ParticleMode::Minimal => "Minimal",
            }
        );
        let vignette_label = if self.vignette {
            "Vignette: ON"
        } else {
            "Vignette: OFF"
        };
        let autosave_label = if self.show_autosave_indicator {
            "Show Autosave Indicator: ON"
        } else {
            "Show Autosave Indicator: OFF"
        };
        let rows: Vec<OptRow> = vec![
            OptRow::Header("Display"),
            OptRow::Big("Fullscreen Resolution: Current"),
            OptRow::Pair(&mf, vsync_label),
            OptRow::Pair("Inactivity FPS Limit: 1 minute", &gui_label),
            OptRow::Pair(fullscreen_label, "Exclusive Fullscreen: OFF"),
            OptRow::Pair("Brightness: 50%", "Graphics Backend: Default"),
            OptRow::Header("Quality"),
            OptRow::Big("Graphics: Fancy"),
            OptRow::Pair("Biome Blend: 5x5", &rd),
            // TODO: static stub, not wired (see mesher::enqueue).
            OptRow::Pair("Prioritize Chunk Updates: None", &sd),
            OptRow::Pair("Smooth Lighting: ON", &clouds_label),
            OptRow::PairLeft(&cd),
            OptRow::Pair(&particles_label, "Mipmap Levels: 4"),
            OptRow::Pair("Entity Shadows: ON", &ed),
            OptRow::Pair("Menu Background Blur: 50%", "Cloud Range: 128"),
            OptRow::Pair("Cutout Leaves: Fancy", "Improved Transparency: OFF"),
            OptRow::Pair("Texture Filtering: None", "Max Anisotropy: 1"),
            OptRow::PairLeft("Weather Radius: 10"),
            OptRow::Header("Preferences"),
            OptRow::Pair(autosave_label, vignette_label),
            OptRow::Pair(&attack_label, "Chunk Fade-in: 1.0s"),
        ];
        let rd_frac = ((self.render_distance as f32 - 2.0) / (rd_max as f32 - 2.0)).clamp(0.0, 1.0);
        let cd_frac = ((self.chunk_detail as f32 - 8.0) / 40.0).clamp(0.0, 1.0);
        let sd_frac = (self.simulation_distance as f32 - 5.0) / 27.0;
        let mf_frac = (self.max_framerate as f32 - 10.0) / 250.0;
        let sliders: &[(&str, f32)] = &[
            ("Render Distance:", rd_frac),
            (
                ENTITY_DISTANCE,
                (self.entity_distance_percent - 50) as f32 / 450.0,
            ),
            ("Chunk Detail:", cd_frac),
            ("Simulation Distance:", sd_frac),
            ("Max Framerate:", mf_frac),
        ];
        let disabled = &[
            "Fullscreen Resolution:",
            "Inactivity FPS Limit:",
            "Exclusive Fullscreen:",
            "Brightness:",
            "Graphics Backend:",
            "Graphics:",
            "Biome Blend:",
            "Prioritize Chunk Updates:",
            "Simulation Distance:",
            "Smooth Lighting:",
            "Mipmap Levels:",
            "Entity Shadows:",
            "Menu Background Blur:",
            "Cloud Range:",
            "Cutout Leaves:",
            "Improved Transparency:",
            "Texture Filtering:",
            "Max Anisotropy:",
            "Weather Radius:",
            "Chunk Fade-in:",
        ];
        self.build_options_grid(
            sw,
            sh,
            input,
            "Video Settings",
            Screen::Options,
            &rows,
            &[],
            sliders,
            disabled,
            true,
            &[],
            text_width_fn,
        )
    }

    pub(super) fn build_options_controls(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        let sensitivity_label = if self.sensitivity <= 0.0 {
            "Sensitivity: *yawn*".to_string()
        } else if self.sensitivity >= 1.0 {
            "Sensitivity: HYPERSPEED!!!".to_string()
        } else {
            format!("Sensitivity: {}%", (self.sensitivity * 200.0) as u32)
        };
        let invert = if self.invert_mouse {
            "Invert Mouse: ON"
        } else {
            "Invert Mouse: OFF"
        };
        let rows: Vec<OptRow> = vec![
            OptRow::Pair(&sensitivity_label, invert),
            OptRow::Pair("Auto-Jump: ON", "Operator Items Tab: OFF"),
            OptRow::Pair("Key Binds...", "Mouse Settings..."),
            OptRow::Pair("Sneak: Toggle", "Sprint: Hold"),
        ];
        let nav: &[(&str, Screen)] = &[("Key Binds...", Screen::OptionsKeybinds)];
        let sliders: &[(&str, f32)] = &[("Sensitivity:", self.sensitivity)];
        let disabled = &[
            "Auto-Jump:",
            "Operator Items Tab:",
            "Key Binds...",
            "Mouse Settings...",
            "Sneak:",
            "Sprint:",
        ];
        self.build_options_grid(
            sw,
            sh,
            input,
            "Controls",
            Screen::Options,
            &rows,
            nav,
            sliders,
            disabled,
            true,
            &[],
            text_width_fn,
        )
    }

    pub(super) fn build_options_chat(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        let o = self.chat_options;
        let on_off = |v: bool| if v { "ON" } else { "OFF" };
        let chat = format!("{CHAT_VISIBILITY} {}", o.visibility.label());
        let colors = format!("{CHAT_COLORS} {}", on_off(o.colors));
        let links = format!("{CHAT_LINKS} {}", on_off(o.links));
        let prompt = format!("{CHAT_LINKS_PROMPT} {}", on_off(o.links_prompt));
        let opacity = chat_opacity_label(o.opacity);
        let bg_opacity = percent_label(TEXT_BACKGROUND_OPACITY, o.text_background_opacity.into());
        let scale = chat_scale_label(o.scale);
        let spacing = percent_label(CHAT_LINE_SPACING, o.line_spacing.into());
        let delay = chat_delay_label(o.delay_secs);
        let width = format!("{CHAT_WIDTH} {}px", o.width_px() as i32);
        let focused = format!("{CHAT_HEIGHT_FOCUSED} {}px", o.height_px(true) as i32);
        let unfocused = format!("{CHAT_HEIGHT_UNFOCUSED} {}px", o.height_px(false) as i32);
        let suggestions = format!("{AUTO_SUGGESTIONS} {}", on_off(o.auto_suggestions));
        let hide_matched = format!("{HIDE_MATCHED_NAMES} ON");
        let secure = format!("{ONLY_SHOW_SECURE_CHAT} {}", on_off(o.only_secure));
        let drafts = format!("{SAVE_CHAT_DRAFTS} {}", on_off(o.save_drafts));
        let rows: Vec<OptRow> = vec![
            OptRow::Pair(&chat, &colors),
            OptRow::Pair(&links, &prompt),
            OptRow::Pair(&opacity, &bg_opacity),
            OptRow::Pair(&scale, &spacing),
            OptRow::Pair(&delay, &width),
            OptRow::Pair(&focused, &unfocused),
            OptRow::Pair("Narrator: OFF", &suggestions),
            OptRow::Pair(&hide_matched, "Reduced Debug Info: OFF"),
            OptRow::Pair(&secure, &drafts),
        ];
        let sliders: &[(&str, f32)] = &[
            (CHAT_OPACITY, o.opacity),
            (TEXT_BACKGROUND_OPACITY, o.text_background_opacity),
            (CHAT_SCALE, o.scale),
            (CHAT_LINE_SPACING, o.line_spacing),
            (CHAT_DELAY, chat_delay_slider(o.delay_secs)),
            (CHAT_WIDTH, o.width),
            (CHAT_HEIGHT_FOCUSED, o.height_focused),
            (CHAT_HEIGHT_UNFOCUSED, o.height_unfocused),
        ];
        // TODO: the menu tooltip wraps at 40% of the screen width and drops
        // `\n`; vanilla `Tooltip.create` splits lines and wraps at 170px.
        let tooltips: &[(&str, &str)] = &[
            (
                HIDE_MATCHED_NAMES,
                "3rd-party Servers may send chat messages in non-standard formats.\nWith this option on, hidden players will be matched based on chat sender names.",
            ),
            (
                ONLY_SHOW_SECURE_CHAT,
                "Only display messages from other players that can be verified to have been sent by that player, and have not been modified.",
            ),
            (
                SAVE_CHAT_DRAFTS,
                "Unsent messages will be saved and can be sent the next time chat is opened.",
            ),
        ];
        let disabled = &["Narrator:", HIDE_MATCHED_NAMES, "Reduced Debug Info:"];
        self.build_options_grid(
            sw,
            sh,
            input,
            "Chat Settings",
            self.settings_back.clone_screen(),
            &rows,
            &[],
            sliders,
            disabled,
            true,
            tooltips,
            text_width_fn,
        )
    }

    pub(super) fn build_options_accessibility(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        let fov_effect_label = if self.fov_effect_scale <= 0.0 {
            "FOV Effects: OFF".to_string()
        } else {
            format!("FOV Effects: {}%", (self.fov_effect() * 100.0).round())
        };
        let damage_tilt_label = if self.damage_tilt_strength <= 0.0 {
            "Damage Tilt: OFF".to_string()
        } else {
            format!(
                "Damage Tilt: {}%",
                (self.damage_tilt_strength * 100.0).round()
            )
        };
        let o = self.chat_options;
        let bg_opacity = percent_label(TEXT_BACKGROUND_OPACITY, o.text_background_opacity.into());
        let chat_opacity = chat_opacity_label(o.opacity);
        let spacing = percent_label(CHAT_LINE_SPACING, o.line_spacing.into());
        let delay = chat_delay_label(o.delay_secs);
        let rows: Vec<OptRow> = vec![
            OptRow::Pair("Narrator: OFF", self.show_subtitles_label()),
            OptRow::Pair("High Contrast: OFF", "Menu Background Blur: 50%"),
            OptRow::Pair(&bg_opacity, "Background for Chat Only: OFF"),
            OptRow::Pair(&chat_opacity, &spacing),
            OptRow::Pair(&delay, "Notification Time: 10.0s"),
            OptRow::Pair(self.view_bobbing_label(), "Distortion Effects: 100%"),
            OptRow::Pair(&fov_effect_label, "Darkness Pulsing: 100%"),
            OptRow::Pair(&damage_tilt_label, "Glint Speed: 100%"),
            OptRow::Pair("Glint Strength: 100%", "Hide Lightning Flashes: OFF"),
            OptRow::Pair("Dark Loading Screen: OFF", "Panorama Scroll Speed: 100%"),
            OptRow::Pair("Hide Splash Texts: OFF", "Narrator Hotkey: ON"),
            OptRow::Pair("Rotate with Minecart: OFF", "High Contrast Outlines: OFF"),
        ];
        let back = self.settings_back.clone_screen();
        let sliders: &[(&str, f32)] = &[
            (TEXT_BACKGROUND_OPACITY, o.text_background_opacity),
            (CHAT_OPACITY, o.opacity),
            (CHAT_LINE_SPACING, o.line_spacing),
            (CHAT_DELAY, chat_delay_slider(o.delay_secs)),
            ("FOV Effects:", self.fov_effect_scale),
            ("Damage Tilt:", self.damage_tilt_strength),
        ];
        let disabled = &[
            "Narrator:",
            "High Contrast:",
            "Menu Background Blur:",
            "Background for Chat Only:",
            "Notification Time:",
            "Distortion Effects:",
            "Darkness Pulsing:",
            "Glint Speed:",
            "Glint Strength:",
            "Hide Lightning Flashes:",
            "Dark Loading Screen:",
            "Panorama Scroll Speed:",
            "Hide Splash Texts:",
            "Narrator Hotkey:",
            "Rotate with Minecart:",
            "High Contrast Outlines:",
        ];
        self.build_options_grid(
            sw,
            sh,
            input,
            "Accessibility Settings",
            back,
            &rows,
            &[],
            sliders,
            disabled,
            true,
            &[],
            text_width_fn,
        )
    }

    pub(super) fn build_options_music(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        let pct = |v: f32| -> String {
            let p = (v * 100.0).round() as u32;
            if p == 0 {
                "OFF".to_string()
            } else {
                format!("{p}%")
            }
        };
        let master = format!("Master Volume: {}", pct(self.master_volume));
        let music = format!("Music: {}", pct(self.music_volume));
        let jukebox = format!("Jukebox/Note Blocks: {}", pct(self.jukebox_volume));
        let weather = format!("Weather: {}", pct(self.weather_volume));
        let blocks = format!("Blocks: {}", pct(self.blocks_volume));
        let hostile = format!("Hostile Creatures: {}", pct(self.hostile_volume));
        let friendly = format!("Friendly Creatures: {}", pct(self.friendly_volume));
        let players = format!("Players: {}", pct(self.players_volume));
        let ambient = format!("Ambient/Environment: {}", pct(self.ambient_volume));
        let voice = format!("Voice/Speech: {}", pct(self.voice_volume));
        let ui = format!("UI: {}", pct(self.ui_volume));
        let rows: Vec<OptRow> = vec![
            OptRow::Big(&master),
            OptRow::Pair(&music, &jukebox),
            OptRow::Pair(&weather, &blocks),
            OptRow::Pair(&hostile, &friendly),
            OptRow::Pair(&players, &ambient),
            OptRow::Pair(&voice, &ui),
            OptRow::Big("Device: Default"),
            OptRow::Pair(self.show_subtitles_label(), "Directional Audio: OFF"),
            OptRow::Pair("Music Frequency: Normal", "Music Toast: ON"),
        ];
        let sliders: &[(&str, f32)] = &[
            ("Master Volume:", self.master_volume),
            ("Music:", self.music_volume),
            ("Jukebox/Note Blocks:", self.jukebox_volume),
            ("Weather:", self.weather_volume),
            ("Blocks:", self.blocks_volume),
            ("Hostile Creatures:", self.hostile_volume),
            ("Friendly Creatures:", self.friendly_volume),
            ("Players:", self.players_volume),
            ("Ambient/Environment:", self.ambient_volume),
            ("Voice/Speech:", self.voice_volume),
            ("UI:", self.ui_volume),
        ];
        let disabled = &[
            "Device:",
            "Directional Audio:",
            "Music Frequency:",
            "Music Toast:",
        ];
        self.build_options_grid(
            sw,
            sh,
            input,
            "Music & Sounds",
            Screen::Options,
            &rows,
            &[],
            sliders,
            disabled,
            true,
            &[],
            text_width_fn,
        )
    }

    pub(super) fn build_options_skin(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        let cape = if self.skin_cape {
            "Cape: ON"
        } else {
            "Cape: OFF"
        };
        let jacket = if self.skin_jacket {
            "Jacket: ON"
        } else {
            "Jacket: OFF"
        };
        let left_sleeve = if self.skin_left_sleeve {
            "Left Sleeve: ON"
        } else {
            "Left Sleeve: OFF"
        };
        let right_sleeve = if self.skin_right_sleeve {
            "Right Sleeve: ON"
        } else {
            "Right Sleeve: OFF"
        };
        let left_pants = if self.skin_left_pants {
            "Left Pants Leg: ON"
        } else {
            "Left Pants Leg: OFF"
        };
        let right_pants = if self.skin_right_pants {
            "Right Pants Leg: ON"
        } else {
            "Right Pants Leg: OFF"
        };
        let hat = if self.skin_hat { "Hat: ON" } else { "Hat: OFF" };
        let main_hand = if self.skin_main_hand_right {
            "Main Hand: Right"
        } else {
            "Main Hand: Left"
        };
        let rows: Vec<OptRow> = vec![
            OptRow::Pair(cape, jacket),
            OptRow::Pair(left_sleeve, right_sleeve),
            OptRow::Pair(left_pants, right_pants),
            OptRow::Pair(hat, main_hand),
        ];
        let disabled: &[&str] = &[];
        self.build_options_grid(
            sw,
            sh,
            input,
            "Skin Customization",
            Screen::Options,
            &rows,
            &[],
            &[],
            disabled,
            true,
            &[],
            text_width_fn,
        )
    }

    pub(super) fn build_options_online(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        let online_status_label = if self.show_online_status {
            "Show Online Status: ON"
        } else {
            "Show Online Status: OFF"
        };
        let current_server_label = if self.show_current_server {
            "Show Current Server: ON"
        } else {
            "Show Current Server: OFF"
        };
        let rows: Vec<OptRow> = vec![
            OptRow::Pair("Realms Notifications: ON", "Allow Server Listings: ON"),
            OptRow::Pair(online_status_label, current_server_label),
        ];
        let tooltips: &[(&str, &str)] = &[
            (
                "Realms Notifications:",
                "Receive notifications about Realms updates",
            ),
            (
                "Allow Server Listings:",
                "Allow servers to list your name in their player list",
            ),
            (
                "Show Online Status:",
                "Allow friends to see when you're online",
            ),
            (
                "Show Current Server:",
                "Allow friends to see which server you're on",
            ),
        ];
        let disabled = &[
            "Realms Notifications:",
            "Allow Server Listings:",
            "Show Online Status:",
            "Show Current Server:",
        ];
        self.build_options_grid(
            sw,
            sh,
            input,
            "Online Options...",
            Screen::Options,
            &rows,
            &[],
            &[],
            disabled,
            true,
            tooltips,
            text_width_fn,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_options_grid(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        title: &str,
        back: Screen,
        rows: &[OptRow],
        nav: &[(&str, Screen)],
        sliders: &[(&'static str, f32)],
        disabled: &[&str],
        header_footer: bool,
        tooltips: &[(&str, &str)],
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        if input.escape {
            self.set_screen(back.clone_screen());
            return empty_result(2.0);
        }

        let gs = crate::ui::hud::gui_scale(sw, sh, self.gui_scale_setting);
        let fs = common::FONT_SIZE * gs;
        let btn_h = common::BTN_H * gs;
        let big_w = 310.0 * gs;
        let small_w = 150.0 * gs;
        let row_h = 25.0 * gs;
        let lh = 9.0 * gs;
        let btn_dy = (row_h - btn_h) / 2.0;
        let cx = sw / 2.0;
        let left_x = cx - 155.0 * gs;
        let right_x = left_x + 160.0 * gs;
        let cursor = input.cursor;
        let clicked = input.clicked;

        let mut elements = Vec::new();
        let mut any_hovered = false;
        let mut any_clicked = false;

        let (content_top, content_bottom, done_y);

        if header_footer {
            let header_h = HEADER_FOOTER_H * gs;
            let footer_h = HEADER_FOOTER_H * gs;
            let sep_h = 2.0 * gs;
            content_top = header_h + sep_h;
            content_bottom = sh - footer_h - sep_h;
            done_y = sh - footer_h + (footer_h - btn_h) / 2.0;

            push_menu_backdrop(
                &mut elements,
                0.0,
                content_top,
                sw,
                content_bottom - content_top,
                gs,
            );

            elements.push(MenuElement::Text {
                x: cx,
                y: (header_h - fs) / 2.0,
                text: option_text(title),
                scale: fs,
                color: WHITE,
                centered: true,
            });
            elements.push(MenuElement::Image {
                x: 0.0,
                y: header_h,
                w: sw,
                h: sep_h,
                sprite: SpriteId::HeaderSeparator,
                tint: WHITE,
            });
            elements.push(MenuElement::Image {
                x: 0.0,
                y: content_bottom,
                w: sw,
                h: sep_h,
                sprite: SpriteId::FooterSeparator,
                tint: WHITE,
            });
        } else {
            let title_y = 15.0 * gs;
            let done_pad = 8.0 * gs;
            content_top = title_y + fs + 10.0 * gs;
            done_y = sh - btn_h - done_pad;
            content_bottom = done_y;

            common::push_overlay(&mut elements, sw, sh, 0.4);

            elements.push(MenuElement::Text {
                x: cx,
                y: title_y,
                text: option_text(title),
                scale: fs,
                color: WHITE,
                centered: true,
            });
        }

        let content_pad = if header_footer { 4.0 * gs } else { 0.0 };
        // Vertical advance per row (vanilla OptionsList: 25px pitch; headers pad
        // above).
        let header_pad_top = |i: usize| if i == 0 { 0.0 } else { 2.0 * lh };
        let row_advance = |i: usize, row: &OptRow| -> f32 {
            match row {
                OptRow::Header(_) => header_pad_top(i) + lh + 4.0 * gs,
                _ => row_h,
            }
        };
        let grid_h: f32 = rows
            .iter()
            .enumerate()
            .map(|(i, row)| row_advance(i, row))
            .sum();
        let content_h = content_bottom - content_top;
        let scrollable = header_footer && grid_h + content_pad > content_h;
        if scrollable {
            let max_scroll = (grid_h + content_pad - content_h).max(0.0);
            if common::hit_test(cursor, [0.0, content_top, sw, content_h]) {
                self.scroll_offset -= input.scroll_delta * 20.0 * gs;
            }
            self.scroll_offset = self.scroll_offset.clamp(0.0, max_scroll);
        } else {
            self.scroll_offset = 0.0;
        }
        let scroll = if scrollable { self.scroll_offset } else { 0.0 };
        let top_y = if header_footer {
            content_top + content_pad - scroll
        } else {
            content_top + (content_h - grid_h) / 2.0
        };
        let mut slider_results: Vec<(&str, f32)> = Vec::new();
        let entity_prefix = entity_distance_prefix();
        let label_scroll = common::LabelScroll {
            text_width_fn,
            time_secs: self.created.elapsed().as_secs_f64(),
        };

        if header_footer {
            elements.push(MenuElement::ScissorPush {
                x: 0.0,
                y: content_top,
                w: sw,
                h: content_bottom - content_top,
            });
        }

        // Rewording a label would silently re-enable its control, so catch a
        // prefix that no longer matches anything.
        #[cfg(debug_assertions)]
        {
            for p in disabled {
                debug_assert!(
                    rows.iter().flat_map(row_labels).any(|l| l.starts_with(p)),
                    "{title}: disabled prefix {p:?} matches no row",
                );
            }
        }

        self.focus_advance(input);
        let mut ctx = self.make_focus_ctx(input);

        let mut y_cursor = top_y;
        for (i, row) in rows.iter().enumerate() {
            let by = y_cursor + btn_dy;
            let mut widgets: Vec<(&str, f32, f32)> = Vec::new();
            match row {
                OptRow::Header(title) => {
                    let pad_top = header_pad_top(i);
                    elements.push(MenuElement::Text {
                        x: left_x,
                        y: y_cursor + pad_top + (lh - fs) / 2.0,
                        text: option_text(title),
                        scale: fs,
                        color: WHITE,
                        centered: false,
                    });
                }
                OptRow::Big(label) => widgets.push((*label, left_x, big_w)),
                OptRow::Pair(a, b) => {
                    widgets.push((*a, left_x, small_w));
                    widgets.push((*b, right_x, small_w));
                }
                OptRow::PairLeft(a) => widgets.push((*a, left_x, small_w)),
            }
            for (label, bx, bw) in widgets {
                let display_label = option_text(label);
                let enabled = option_enabled(label, disabled);
                if let Some((prefix, value)) = sliders
                    .iter()
                    .find(|(p, _)| slider_matches(label, p, &entity_prefix))
                {
                    let hovered = enabled && common::hit_test(cursor, [bx, by, bw, btn_h]);
                    let prev_focus = ctx.focus;
                    let focused = ctx.focused(enabled, hovered);
                    // `setFocused` only re-arms editing when focus moves, so
                    // re-clicking a locked slider leaves it locked.
                    if focused && ctx.focus != prev_focus {
                        self.slider_can_change_value = true;
                    }
                    if focused && ctx.activate {
                        self.slider_can_change_value = !self.slider_can_change_value;
                    }
                    let is_active = self.active_slider == Some(*prefix);
                    let result = common::push_slider(
                        &mut elements,
                        cursor,
                        input.clicked,
                        input.mouse_held,
                        bx,
                        by,
                        bw,
                        btn_h,
                        gs,
                        fs,
                        &display_label,
                        *value,
                        enabled,
                        focused,
                        self.slider_can_change_value,
                        is_active,
                        &label_scroll,
                    );
                    any_hovered |= result.hovered;
                    if result.dragging {
                        self.active_slider = Some(*prefix);
                    }
                    if let Some(v) = result.new_value {
                        slider_results.push((prefix, v));
                    }
                    let steps = input.arrow_steps();
                    if focused && self.slider_can_change_value && steps != 0 {
                        let step = self.slider_key_step(prefix, bw, gs);
                        slider_results
                            .push((prefix, (*value + steps as f32 * step).clamp(0.0, 1.0)));
                    }
                    // `onRelease`: the click sound the press skipped. Also
                    // covers a press and release inside one frame.
                    if !input.mouse_held && (is_active || result.dragging) {
                        self.active_slider = None;
                        any_clicked = true;
                    }
                    continue;
                }

                let hit = common::hit_test(cursor, [bx, by, bw, btn_h]);
                let h = enabled && hit;
                let focused = ctx.focused(enabled, h);
                let draw_cursor = helpers::focus_cursor(focused, h, bx, by, bw, btn_h, cursor);
                common::push_button_scrolling(
                    &mut elements,
                    draw_cursor,
                    bx,
                    by,
                    bw,
                    btn_h,
                    gs,
                    fs,
                    &display_label,
                    enabled,
                    &label_scroll,
                );
                any_hovered |= h;
                // Vanilla keys tooltips off `isHovered`, which ignores `active`
                // (`AbstractWidget.extractTooltipForNextRenderPass`).
                if hit && let Some((_, tip)) = tooltips.iter().find(|(p, _)| label.starts_with(p)) {
                    common::push_tooltip(&mut elements, cursor, sw, sh, gs, &option_tooltip(tip));
                }
                if (clicked && h) || (focused && ctx.activate) {
                    any_clicked = true;
                    if let Some((_, target)) = nav.iter().find(|(l, _)| *l == label) {
                        if matches!(target, Screen::OptionsResourcePacks) {
                            self.rescan_packs = true;
                        }
                        self.set_screen(target.clone_screen());
                        if matches!(self.screen, Screen::OptionsResourcePacks) {
                            self.focused_field = Some(0);
                            self.pack_search.set_focused(true);
                        }
                    }
                    if matches!(self.screen, Screen::OptionsLanguage) {
                        let locale = if label.starts_with("English (US)") {
                            Some("en_us")
                        } else if label.starts_with("日本語 (日本)") {
                            Some("ja_jp")
                        } else {
                            None
                        };
                        if let Some(locale) = locale
                            && self.locale != locale
                            && crate::lang::set_locale(locale)
                        {
                            self.locale = supported_locale(locale);
                            self.save_settings();
                        }
                    }
                    if label.starts_with("GUI Scale:") {
                        let max = crate::ui::hud::max_gui_scale(sw, sh);
                        self.gui_scale_setting = (self.gui_scale_setting + 1) % (max + 1);
                        self.save_settings();
                    }
                    if label.starts_with("Fullscreen:") {
                        self.set_display_mode(self.display_mode.cycle());
                    }
                    if label.starts_with("Particles:") {
                        self.particle_status =
                            crate::particle::ParticleMode::from_u8(self.particle_status)
                                .cycle()
                                .to_u8();
                        self.save_settings();
                    }
                    if label.starts_with("Clouds:") {
                        self.cloud_mode = self.cloud_mode.cycle();
                        self.save_settings();
                    }
                    if label.starts_with("Attack Indicator:") {
                        self.attack_indicator = self.attack_indicator.cycle();
                        self.save_settings();
                    }
                    if label.starts_with("Invert Mouse:") {
                        self.invert_mouse = !self.invert_mouse;
                        self.save_settings();
                    }
                    if label.starts_with("View Bobbing:") {
                        self.view_bobbing = !self.view_bobbing;
                        self.save_settings();
                    }
                    if label.starts_with("Show Autosave Indicator:") {
                        self.show_autosave_indicator = !self.show_autosave_indicator;
                        self.save_settings();
                    }
                    if label.starts_with("VSync:") {
                        self.vsync = !self.vsync;
                        self.save_settings();
                    }
                    if label.starts_with("Show Subtitles:") {
                        self.show_subtitles = !self.show_subtitles;
                        self.save_settings();
                    }
                    if label.starts_with("Vignette:") {
                        self.vignette = !self.vignette;
                        self.save_settings();
                    }
                    if label.starts_with("Show Online Status:") {
                        self.show_online_status = !self.show_online_status;
                        self.save_settings();
                    }
                    if label.starts_with("Show Current Server:") {
                        self.show_current_server = !self.show_current_server;
                        self.save_settings();
                    }
                    if label.starts_with(CHAT_VISIBILITY) {
                        self.chat_options.visibility = self.chat_options.visibility.cycle();
                        self.save_settings();
                    }
                    if label.starts_with(CHAT_COLORS) {
                        self.chat_options.colors = !self.chat_options.colors;
                        self.save_settings();
                    }
                    if label.starts_with(CHAT_LINKS) {
                        self.chat_options.links = !self.chat_options.links;
                        self.save_settings();
                    }
                    if label.starts_with(CHAT_LINKS_PROMPT) {
                        self.chat_options.links_prompt = !self.chat_options.links_prompt;
                        self.save_settings();
                    }
                    if label.starts_with(AUTO_SUGGESTIONS) {
                        self.chat_options.auto_suggestions = !self.chat_options.auto_suggestions;
                        self.save_settings();
                    }
                    if label.starts_with(ONLY_SHOW_SECURE_CHAT) {
                        self.chat_options.only_secure = !self.chat_options.only_secure;
                        self.save_settings();
                    }
                    if label.starts_with(SAVE_CHAT_DRAFTS) {
                        self.chat_options.save_drafts = !self.chat_options.save_drafts;
                        self.save_settings();
                    }
                    if label.starts_with("Cape:") {
                        self.skin_cape = !self.skin_cape;
                        self.save_settings();
                    }
                    if label.starts_with("Jacket:") {
                        self.skin_jacket = !self.skin_jacket;
                        self.save_settings();
                    }
                    if label.starts_with("Left Sleeve:") {
                        self.skin_left_sleeve = !self.skin_left_sleeve;
                        self.save_settings();
                    }
                    if label.starts_with("Right Sleeve:") {
                        self.skin_right_sleeve = !self.skin_right_sleeve;
                        self.save_settings();
                    }
                    if label.starts_with("Left Pants Leg:") {
                        self.skin_left_pants = !self.skin_left_pants;
                        self.save_settings();
                    }
                    if label.starts_with("Right Pants Leg:") {
                        self.skin_right_pants = !self.skin_right_pants;
                        self.save_settings();
                    }
                    if label.starts_with("Hat:") {
                        self.skin_hat = !self.skin_hat;
                        self.save_settings();
                    }
                    if label.starts_with("Main Hand:") {
                        self.skin_main_hand_right = !self.skin_main_hand_right;
                        self.save_settings();
                    }
                }
            }
            y_cursor += row_advance(i, row);
        }

        for (prefix, value) in &slider_results {
            let v = *value;
            let span = self.discrete_slider_span(prefix).unwrap_or(1.0);
            match *prefix {
                "Render Distance:" => self.render_distance = (2.0 + v * span).round() as u32,
                ENTITY_DISTANCE => {
                    self.entity_distance_percent = 50 + 25 * (v * span).round() as u32
                }
                "Chunk Detail:" => self.chunk_detail = (8.0 + v * span).round() as u32,
                "Simulation Distance:" => {
                    self.simulation_distance = (5.0 + v * span).round() as u32
                }
                "Max Framerate:" => self.max_framerate = 10 + 10 * (v * span).round() as u32,
                "FOV:" => self.fov = (30.0 + v * span).round() as u32,
                "FOV Effects:" => self.fov_effect_scale = v,
                "Damage Tilt:" => self.damage_tilt_strength = v,
                "Sensitivity:" => self.sensitivity = v,
                "Master Volume:" => self.master_volume = v,
                "Music:" => self.music_volume = v,
                "Jukebox/Note Blocks:" => self.jukebox_volume = v,
                "Weather:" => self.weather_volume = v,
                "Blocks:" => self.blocks_volume = v,
                "Hostile Creatures:" => self.hostile_volume = v,
                "Friendly Creatures:" => self.friendly_volume = v,
                "Players:" => self.players_volume = v,
                "Ambient/Environment:" => self.ambient_volume = v,
                "Voice/Speech:" => self.voice_volume = v,
                "UI:" => self.ui_volume = v,
                CHAT_OPACITY => self.chat_options.opacity = v,
                TEXT_BACKGROUND_OPACITY => self.chat_options.text_background_opacity = v,
                CHAT_SCALE => self.chat_options.scale = v,
                CHAT_LINE_SPACING => self.chat_options.line_spacing = v,
                CHAT_DELAY => self.chat_options.delay_secs = chat_delay_from_slider(v),
                CHAT_WIDTH => self.chat_options.width = v,
                CHAT_HEIGHT_FOCUSED => self.chat_options.height_focused = v,
                CHAT_HEIGHT_UNFOCUSED => self.chat_options.height_unfocused = v,
                _ => continue,
            }
            self.settings_dirty = true;
        }

        if header_footer {
            elements.push(MenuElement::ScissorPop);
        }

        if scrollable {
            let max_scroll = (grid_h + content_pad - content_h).max(0.001);
            let track_w = 6.0 * gs;
            let track_x = sw - track_w - 2.0 * gs;
            let thumb_frac = content_h / (grid_h + content_pad);
            let thumb_h = (content_h * thumb_frac).max(8.0 * gs);
            let thumb_y = content_top + (scroll / max_scroll) * (content_h - thumb_h);
            elements.push(MenuElement::NineSlice {
                x: track_x,
                y: content_top,
                w: track_w,
                h: content_h,
                sprite: SpriteId::ScrollerBackground,
                border: 1.0 * gs,
                tint: WHITE,
            });
            elements.push(MenuElement::NineSlice {
                x: track_x,
                y: thumb_y,
                w: track_w,
                h: thumb_h,
                sprite: SpriteId::Scroller,
                border: 1.0 * gs,
                tint: WHITE,
            });
        }

        let done_w = 200.0 * gs;
        let done_h = common::hit_test(cursor, [cx - done_w / 2.0, done_y, done_w, btn_h]);
        let done_focused = ctx.focused(true, done_h);
        let done_cursor = helpers::focus_cursor(
            done_focused,
            done_h,
            cx - done_w / 2.0,
            done_y,
            done_w,
            btn_h,
            cursor,
        );
        common::push_button_scrolling(
            &mut elements,
            done_cursor,
            cx - done_w / 2.0,
            done_y,
            done_w,
            btn_h,
            gs,
            fs,
            crate::lang::translate("gui.done").unwrap_or(crate::lang::ui("Done", "完了")),
            true,
            &label_scroll,
        );
        any_hovered |= done_h;
        if (clicked && done_h) || (done_focused && ctx.activate) {
            any_clicked = true;
            self.set_screen(back);
        }
        self.finish_focus(&ctx);

        MainMenuResult {
            elements,
            action: MenuAction::None,
            cursor_pointer: any_hovered,
            blur: 2.0,
            clicked_button: any_clicked,
        }
    }

    pub(super) fn build_options_resource_packs(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        text_width_fn: common::TextWidthFn,
    ) -> MainMenuResult {
        use crate::resource_pack::{PackSource, ResourcePackManager};

        if input.escape {
            self.pack_search.clear();
            self.set_screen(Screen::Options);
            return empty_result(2.0);
        }

        if self.rescan_packs {
            self.rescan_packs = false;
            self.available_packs = ResourcePackManager::scan_local_packs_at(&self.packs_dir);
            for pack in &mut self.available_packs {
                pack.enabled = self.resource_pack_selection.contains(&pack.name);
            }
            let server_packs: Vec<_> = self
                .active_packs
                .iter()
                .filter(|pack| pack.source == PackSource::Server)
                .cloned()
                .collect();
            self.active_packs.clear();
            for name in &self.resource_pack_selection {
                if let Some(pack) = self.available_packs.iter().find(|pack| &pack.name == name) {
                    self.active_packs.push(pack.clone());
                }
            }
            self.active_packs.extend(server_packs);
        }

        self.cycle_fields(input, 1);

        let gs = crate::ui::hud::gui_scale(sw, sh, self.gui_scale_setting);
        let fs = common::FONT_SIZE * gs;
        let btn_h = common::BTN_H * gs;
        let gap = BTN_GAP * gs;
        let cx = sw / 2.0;
        let cursor = input.cursor;
        let clicked = input.clicked;

        let mut elements = Vec::new();
        let mut any_hovered = false;

        common::push_overlay(&mut elements, sw, sh, 0.4);

        let pad = 4.0 * gs;
        let entry_h = 36.0 * gs;
        let entry_gap = 2.0 * gs;
        let small_fs = 6.0 * gs;
        let list_w = 200.0 * gs;
        let list_gap = 15.0 * gs;
        let left_x = cx - list_gap - list_w;
        let right_x = cx + list_gap;
        let text_x = 34.0 * gs;
        let field_h = 15.0 * gs;
        let hover_color: [f32; 4] = [1.0, 1.0, 1.0, 0.1];
        let drag_text = crate::lang::ui(
            "Drag and drop files into this window to add packs",
            "このウィンドウにファイルをドラッグ＆ドロップしてパックを追加",
        );

        let mut header_y = pad;
        elements.push(MenuElement::Text {
            x: cx,
            y: header_y,
            text: crate::lang::ui("Select Resource Packs", "リソースパックの選択").into(),
            scale: fs,
            color: WHITE,
            centered: true,
        });
        header_y += fs + pad;
        elements.push(MenuElement::Text {
            x: cx,
            y: header_y,
            text: drag_text.into(),
            scale: fs,
            color: COL_DIM,
            centered: true,
        });
        header_y += fs + pad;

        let field_x = cx - list_w / 2.0;
        self.text_field(
            &mut elements,
            TextTarget::PackSearch,
            0,
            input,
            field_x,
            header_y,
            list_w,
            field_h,
            fs,
            gs,
            text_width_fn,
        );
        push_field_hint(
            &mut elements,
            &self.pack_search,
            self.focused_field == Some(0),
            field_x,
            header_y,
            field_h,
            fs,
            gs,
            crate::lang::ui("Search...", "検索..."),
        );
        header_y += field_h + pad;

        let content_top = header_y;
        let footer_h = HEADER_FOOTER_H * gs;
        let content_bottom = sh - footer_h;
        let done_y = sh - footer_h + (footer_h - btn_h) / 2.0;

        elements.push(MenuElement::ScissorPush {
            x: 0.0,
            y: content_top,
            w: sw,
            h: content_bottom - content_top,
        });

        let list_top = content_top + pad;
        let label_h = fs * 1.5;

        elements.push(MenuElement::Text {
            x: left_x + list_w / 2.0,
            y: list_top + (label_h - fs) / 2.0,
            text: crate::lang::ui("Available", "利用可能").into(),
            scale: fs,
            color: WHITE,
            centered: true,
        });
        elements.push(MenuElement::Text {
            x: right_x + list_w / 2.0,
            y: list_top + (label_h - fs) / 2.0,
            text: crate::lang::ui("Selected", "選択済み").into(),
            scale: fs,
            color: WHITE,
            centered: true,
        });

        let entries_top = list_top + label_h + pad;

        let search_lower = self.pack_search.value().to_lowercase();
        let available: Vec<_> = self
            .available_packs
            .iter()
            .filter(|p| {
                !p.enabled
                    && (search_lower.is_empty()
                        || p.name.to_lowercase().contains(&search_lower)
                        || p.description.to_lowercase().contains(&search_lower))
            })
            .cloned()
            .collect();

        let push_entry = |elements: &mut Vec<MenuElement>,
                          any_hovered: &mut bool,
                          panel_x: f32,
                          ey: f32,
                          name: &str,
                          desc: &str,
                          name_color: [f32; 4],
                          compat: crate::resource_pack::PackCompat,
                          interactive: bool|
         -> bool {
            let hovered = interactive && common::hit_test(cursor, [panel_x, ey, list_w, entry_h]);
            if hovered {
                elements.push(MenuElement::Rect {
                    x: panel_x,
                    y: ey,
                    w: list_w,
                    h: entry_h,
                    corner_radius: 0.0,
                    color: hover_color,
                });
            }
            elements.push(MenuElement::Text {
                x: panel_x + text_x,
                y: ey + 4.0 * gs,
                text: name.into(),
                scale: fs,
                color: name_color,
                centered: false,
            });
            elements.push(MenuElement::Text {
                x: panel_x + text_x,
                y: ey + 4.0 * gs + fs + gs,
                text: desc.into(),
                scale: small_fs,
                color: COL_DIM,
                centered: false,
            });
            let (ct, cc) = compat_label(compat);
            elements.push(MenuElement::Text {
                x: panel_x + text_x,
                y: ey + 4.0 * gs + fs + gs + small_fs + gs,
                text: ct.into(),
                scale: small_fs,
                color: cc,
                centered: false,
            });
            *any_hovered |= hovered;
            hovered
        };

        for (i, pack) in available.iter().enumerate() {
            let ey = entries_top + i as f32 * (entry_h + entry_gap);
            if push_entry(
                &mut elements,
                &mut any_hovered,
                left_x,
                ey,
                &pack.name,
                &pack.description,
                WHITE,
                pack.compat,
                true,
            ) && clicked
            {
                self.pack_toggle = Some((pack.name.clone(), true));
                self.reload_assets = true;
                self.resource_pack_selection.push(pack.name.clone());
                self.save_settings();
                if let Some(pack) = self
                    .available_packs
                    .iter_mut()
                    .find(|available| available.name == pack.name)
                {
                    pack.enabled = true;
                    self.active_packs.push(pack.clone());
                }
            }
        }

        let selected: Vec<_> = self.active_packs.clone();
        let default_offset = selected.len() as f32;

        for (i, pack) in selected.iter().enumerate() {
            let ey = entries_top + i as f32 * (entry_h + entry_gap);
            let is_server = pack.source == PackSource::Server;
            let label = if is_server {
                format!("[{}] {}", crate::lang::ui("Server", "サーバー"), pack.name)
            } else {
                pack.name.clone()
            };
            let name_color = if is_server {
                common::COL_DISABLED
            } else {
                WHITE
            };
            if push_entry(
                &mut elements,
                &mut any_hovered,
                right_x,
                ey,
                &label,
                &pack.description,
                name_color,
                pack.compat,
                !is_server,
            ) && clicked
            {
                self.pack_toggle = Some((pack.name.clone(), false));
                self.reload_assets = true;
                self.resource_pack_selection
                    .retain(|selected| selected != &pack.name);
                self.save_settings();
                self.active_packs.retain(|active| {
                    active.source != PackSource::Local || active.name != pack.name
                });
                if let Some(available) = self
                    .available_packs
                    .iter_mut()
                    .find(|available| available.name == pack.name)
                {
                    available.enabled = false;
                }
            }
        }

        push_entry(
            &mut elements,
            &mut any_hovered,
            right_x,
            entries_top + default_offset * (entry_h + entry_gap),
            crate::lang::ui("Default", "デフォルト"),
            crate::lang::ui(
                "The default look and feel of Minecraft",
                "Minecraftの標準の外観",
            ),
            WHITE,
            crate::resource_pack::PackCompat::Compatible,
            false,
        );

        elements.push(MenuElement::ScissorPop);

        let btn_w = 150.0 * gs;
        let h = common::push_button(
            &mut elements,
            cursor,
            cx - btn_w - gap / 2.0,
            done_y,
            btn_w,
            btn_h,
            gs,
            fs,
            crate::lang::ui("Open Pack Folder", "パックフォルダーを開く"),
            true,
        );
        any_hovered |= h;
        if clicked && h {
            let _ = open::that_detached(&self.packs_dir);
        }

        let h = common::push_button(
            &mut elements,
            cursor,
            cx + gap / 2.0,
            done_y,
            btn_w,
            btn_h,
            gs,
            fs,
            crate::lang::translate("gui.done").unwrap_or(crate::lang::ui("Done", "完了")),
            true,
        );
        any_hovered |= h;
        if clicked && h {
            self.pack_search.clear();
            self.set_screen(Screen::Options);
        }

        MainMenuResult {
            elements,
            action: MenuAction::None,
            cursor_pointer: any_hovered,
            blur: 2.0,
            clicked_button: false,
        }
    }

    pub(super) fn build_options_stub(
        &mut self,
        sw: f32,
        sh: f32,
        input: &MenuInput,
        title: &str,
        back: Screen,
    ) -> MainMenuResult {
        if input.escape {
            self.set_screen(back.clone_screen());
            return empty_result(2.0);
        }

        let gs = crate::ui::hud::gui_scale(sw, sh, self.gui_scale_setting);
        let cx = sw / 2.0;

        let mut elements = Vec::new();
        let mut any_hovered = false;

        let chrome = push_screen_chrome(&mut elements, sw, sh, gs, title);

        let body_fs = 10.0 * gs;
        elements.push(MenuElement::Text {
            x: cx,
            y: (chrome.content_top + chrome.content_bottom) / 2.0 - body_fs / 2.0,
            text: crate::lang::ui("Coming soon", "準備中").into(),
            scale: body_fs,
            color: COL_DIM,
            centered: true,
        });

        self.focus_advance(input);
        let mut ctx = self.make_focus_ctx(input);
        if push_done_button(
            &mut elements,
            &mut ctx,
            &mut any_hovered,
            input,
            &chrome,
            cx,
            gs,
        ) {
            self.set_screen(back);
        }
        self.finish_focus(&ctx);

        MainMenuResult {
            elements,
            action: MenuAction::None,
            cursor_pointer: any_hovered,
            blur: 2.0,
            clicked_button: ctx.fired,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::text_edit::KeyMods;

    #[test]
    fn disabled_prefixes_match_dynamic_option_labels() {
        let disabled = &["Simulation Distance:", "Graphics:"];

        assert!(!option_enabled("Simulation Distance: 12 chunks", disabled));
        assert!(!option_enabled("Graphics: Fancy", disabled));
        assert!(option_enabled("Render Distance: 12 chunks", disabled));
        assert!(option_enabled("Graphics Backend: Default", disabled));
    }

    #[test]
    fn chat_option_labels_match_vanilla() {
        let o = crate::ui::chat::ChatOptions::default();
        assert_eq!(chat_opacity_label(o.opacity), "Chat Text Opacity: 100%");
        assert_eq!(chat_opacity_label(0.0), "Chat Text Opacity: 10%");
        assert_eq!(
            percent_label(TEXT_BACKGROUND_OPACITY, 0.29_f32.into()),
            "Text Background Opacity: 28%"
        );
        assert_eq!(chat_scale_label(o.scale), "Chat Text Size: 100%");
        assert_eq!(chat_scale_label(0.0), "Chat Text Size: OFF");
        assert_eq!(chat_delay_label(o.delay_secs), "Chat Delay: None");
        assert_eq!(chat_delay_label(0.5), "Chat Delay: 0.5 second(s)");
        assert_eq!(chat_delay_label(6.0), "Chat Delay: 6.0 second(s)");
    }

    #[test]
    fn chat_delay_slider_round_trips_and_steps() {
        let step = 1.0 / CHAT_DELAY_MAX_TENTHS as f32;
        for tenths in 0..=CHAT_DELAY_MAX_TENTHS {
            let secs = tenths as f32 / 10.0;
            let slider = chat_delay_slider(secs);
            assert_eq!(chat_delay_from_slider(slider), secs, "{tenths}");
            let next = chat_delay_from_slider((slider + step).clamp(0.0, 1.0));
            let prev = chat_delay_from_slider((slider - step).clamp(0.0, 1.0));
            let tenths_of = |s: f32| (s * 10.0).round() as i32;
            assert_eq!(tenths_of(next), (tenths + 1).min(CHAT_DELAY_MAX_TENTHS));
            assert_eq!(tenths_of(prev), (tenths - 1).max(0));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn test_slider(
        cursor: (f32, f32),
        mouse_pressed: bool,
        mouse_held: bool,
        enabled: bool,
        focused: bool,
        can_change_value: bool,
        dragging: bool,
    ) -> (common::SliderResult, Vec<MenuElement>) {
        let mut elements = Vec::new();
        let text_width = |_: &str, _: f32| 0.0;
        let scroll = common::LabelScroll {
            text_width_fn: &text_width,
            time_secs: 0.0,
        };
        let result = common::push_slider(
            &mut elements,
            cursor,
            mouse_pressed,
            mouse_held,
            0.0,
            0.0,
            100.0,
            20.0,
            1.0,
            common::FONT_SIZE,
            "FOV: 70",
            0.5,
            enabled,
            focused,
            can_change_value,
            dragging,
            &scroll,
        );
        (result, elements)
    }

    fn sprites(elements: &[MenuElement]) -> Vec<SpriteId> {
        elements
            .iter()
            .filter_map(|e| match e {
                MenuElement::NineSlice { sprite, .. }
                | MenuElement::Image { sprite, .. }
                | MenuElement::CroppedImage { sprite, .. } => Some(*sprite),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn disabled_slider_cannot_hover_or_drag() {
        let (result, _) = test_slider((50.0, 10.0), true, true, false, true, true, true);

        assert!(!result.hovered);
        assert!(!result.dragging);
        assert_eq!(result.new_value, None);
    }

    #[test]
    fn held_mouse_does_not_start_slider_without_press() {
        let (result, _) = test_slider((75.0, 10.0), false, true, true, false, true, false);

        assert!(result.hovered);
        assert!(!result.dragging);
        assert_eq!(result.new_value, None);
    }

    #[test]
    fn focused_slider_highlights_handle_until_locked() {
        let (result, elements) = test_slider((-10.0, -10.0), false, false, true, true, true, false);
        assert!(!result.hovered);
        assert_eq!(
            sprites(&elements),
            [SpriteId::SliderTrack, SpriteId::SliderHandleHover]
        );

        let (_, elements) = test_slider((-10.0, -10.0), false, false, true, true, false, false);
        assert_eq!(
            sprites(&elements),
            [SpriteId::SliderTrackHover, SpriteId::SliderHandle]
        );
    }

    #[test]
    fn active_slider_keeps_drag_capture_outside_its_bounds() {
        let (result, _) = test_slider((150.0, 10.0), false, true, true, true, true, true);

        assert!(!result.hovered);
        assert!(result.dragging);
        assert_eq!(result.new_value, Some(1.0));
    }

    /// `dir` must not exist: settings and the server list fall back to
    /// defaults, and a settings save silently fails instead of writing.
    fn test_menu(dir: &str) -> MainMenu {
        let rt = std::sync::Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("current-thread runtime"),
        );
        let mut menu = MainMenu::new(
            std::path::Path::new(dir),
            rt,
            "tester".into(),
            "26.2".into(),
            None,
        );
        menu.gui_scale_setting = 1;
        menu
    }

    /// A grid of one `Pair` row at 800x600: the left widget spans x 245..395
    /// and the right one 405..555, both at y 300.
    struct Grid {
        menu: MainMenu,
        rows: [OptRow<'static>; 1],
        nav: Vec<(&'static str, Screen)>,
        sliders: Vec<(&'static str, f32)>,
        disabled: Vec<&'static str>,
    }

    impl Grid {
        fn new(left: &'static str, right: &'static str) -> Self {
            Self {
                menu: test_menu("pomme-options-focus-test"),
                rows: [OptRow::Pair(left, right)],
                nav: Vec::new(),
                sliders: Vec::new(),
                disabled: Vec::new(),
            }
        }

        fn with_slider(left: &'static str, right: &'static str, prefix: &'static str) -> Self {
            let mut grid = Self::new(left, right);
            grid.sliders.push((prefix, 0.5));
            grid
        }

        fn frame(&mut self, input: &MenuInput) -> MainMenuResult {
            let text_width = |_: &str, _: f32| 0.0;
            self.menu.build_options_grid(
                800.0,
                600.0,
                input,
                "Test",
                Screen::Main,
                &self.rows,
                &self.nav,
                &self.sliders,
                &self.disabled,
                false,
                &[],
                &text_width,
            )
        }

        fn shows(&mut self, input: &MenuInput, sprite: SpriteId) -> bool {
            sprites(&self.frame(input).elements).contains(&sprite)
        }
    }

    fn click(x: f32) -> MenuInput {
        MenuInput {
            cursor: (x, 300.0),
            clicked: true,
            mouse_held: true,
            ..Default::default()
        }
    }

    fn key(code: KeyCode) -> MenuInput {
        MenuInput {
            events: vec![TextInputEvent::Key {
                code,
                mods: KeyMods {
                    shift: false,
                    ctrl: false,
                    alt: false,
                    super_key: false,
                },
            }],
            ..Default::default()
        }
    }

    fn tab() -> MenuInput {
        MenuInput {
            tab: true,
            ..Default::default()
        }
    }

    #[test]
    fn mouse_focus_persists_until_another_enabled_option_is_clicked() {
        let mut grid = Grid::new("First", "Second");

        grid.frame(&click(300.0));
        assert_eq!(grid.menu.focus, Some(0));

        let hover_second = MenuInput {
            cursor: (450.0, 300.0),
            ..Default::default()
        };
        let result = grid.frame(&hover_second);
        assert_eq!(grid.menu.focus, Some(0));
        let hover_count = sprites(&result.elements)
            .iter()
            .filter(|s| **s == SpriteId::ButtonHover)
            .count();
        assert_eq!(hover_count, 2);

        grid.disabled = vec!["Second"];
        grid.frame(&click(450.0));
        assert_eq!(grid.menu.focus, Some(0));

        grid.disabled.clear();
        grid.frame(&click(450.0));
        assert_eq!(grid.menu.focus, Some(1));
    }

    #[test]
    fn tab_reaches_sliders_and_enter_toggles_keyboard_editing() {
        let mut grid = Grid::with_slider("First", "Sensitivity: 50%", "Sensitivity:");
        let enter = MenuInput {
            enter: true,
            ..Default::default()
        };

        grid.frame(&MenuInput::default());
        grid.frame(&tab());
        assert!(grid.shows(&tab(), SpriteId::SliderHandleHover));
        assert!(!grid.shows(&MenuInput::default(), SpriteId::SliderTrackHover));

        assert!(grid.shows(&enter, SpriteId::SliderTrackHover));
        assert!(!grid.shows(&MenuInput::default(), SpriteId::SliderHandleHover));

        grid.frame(&key(KeyCode::ArrowRight));
        grid.frame(&enter);
        grid.frame(&key(KeyCode::ArrowRight));
        assert_eq!(grid.menu.focus, Some(1));
        // Locked, the first arrow did nothing; unlocked, one step is
        // `1 / (150 - 8)` GUI units.
        assert!((grid.menu.sensitivity - (0.5 + 1.0 / 142.0)).abs() < 1e-6);
    }

    #[test]
    fn japanese_labels_keep_toggle_and_slider_ids() {
        assert_eq!(option_text_for("en_us", "VSync: ON"), "VSync: ON");
        assert_eq!(option_text_for("ja_jp", "VSync: ON"), "垂直同期: オン");
        let mut grid = Grid::new("VSync: ON", "Second");
        grid.frame(&click(300.0));
        assert!(!grid.menu.vsync);
        let mut grid = Grid::with_slider("First", "Entity Distance: 100%", ENTITY_DISTANCE);
        grid.sliders[0].1 = 1.0;
        grid.frame(&click(550.0));
        assert_eq!(grid.menu.entity_distance_percent, 500);
        grid.sliders[0].1 = 0.0;
        grid.frame(&click(408.0));
        assert_eq!(grid.menu.entity_distance_percent, 50);
        assert_eq!(grid.menu.discrete_slider_span(ENTITY_DISTANCE), Some(18.0));
    }

    #[test]
    fn toggles_use_japanese_when_catalog_falls_back_to_english() {
        for (value, english, japanese) in [
            ("ON", "ON", "オン"),
            ("OFF", "OFF", "オフ"),
            ("Off", "OFF", "オフ"),
        ] {
            assert_eq!(localized_option_value(value, japanese, None), japanese);
            assert_eq!(
                localized_option_value(value, japanese, Some(english)),
                japanese
            );
            assert_eq!(
                localized_option_value(value, japanese, Some(japanese)),
                japanese
            );
        }
        assert_eq!(localized_option_value("Auto", "自動", Some("Auto")), "Auto");
    }

    #[test]
    fn translated_entity_distance_keeps_stable_slider_id() {
        let label = "エンティティの描画距離: 100%";
        assert!(slider_matches(
            label,
            ENTITY_DISTANCE,
            "エンティティの描画距離:"
        ));
        let grid = Grid::with_slider("First", "Entity Distance: 100%", ENTITY_DISTANCE);
        assert_eq!(grid.menu.discrete_slider_span(ENTITY_DISTANCE), Some(18.0));
    }

    #[test]
    fn language_screen_click_and_keyboard_switch_and_persist() {
        let dir = std::env::temp_dir().join(format!("pomme-language-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        save_settings(
            &dir,
            &Settings {
                locale: "fr_fr".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        assert_eq!(load_settings(&dir).locale, "en_us");
        let mut menu = test_menu(dir.to_str().unwrap());
        menu.set_screen(Screen::OptionsLanguage);
        let width = |_: &str, _: f32| 0.0;
        menu.build(800.0, 600.0, &MenuInput::default(), width);
        menu.build(
            800.0,
            600.0,
            &MenuInput {
                cursor: (450.0, 50.0),
                ..click(450.0)
            },
            width,
        );
        assert_eq!(load_settings(&dir).locale, "ja_jp");
        assert_eq!(menu.locale, "ja_jp");
        let result = menu.build(800.0, 600.0, &MenuInput::default(), width);
        assert!(
            result
                .elements
                .iter()
                .any(|e| matches!(e, MenuElement::Text { text, .. } if text == "日本語 (日本) ✓"))
        );
        menu.build(800.0, 600.0, &tab(), width);
        menu.build(800.0, 600.0, &tab(), width);
        menu.build(
            800.0,
            600.0,
            &MenuInput {
                enter: true,
                ..Default::default()
            },
            width,
        );
        assert_eq!(menu.locale, "en_us");
        assert_eq!(load_settings(&dir).locale, "en_us");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn entity_distance_slider_steps_and_snaps() {
        let mut grid = Grid::with_slider("First", "Entity Distance: 100%", "Entity Distance:");
        grid.menu.entity_distance_percent = 100;
        grid.sliders[0].1 = 2.0 / 18.0;
        grid.frame(&MenuInput::default());
        grid.frame(&tab());
        grid.frame(&tab());
        grid.frame(&key(KeyCode::ArrowRight));
        assert_eq!(grid.menu.entity_distance_percent, 125);
        grid.sliders[0].1 = 3.0 / 18.0;
        grid.frame(&key(KeyCode::ArrowLeft));
        assert_eq!(grid.menu.entity_distance_percent, 100);
        assert_eq!(
            grid.menu.discrete_slider_span("Entity Distance:"),
            Some(18.0)
        );
        // The right widget spans x=405..555; dragging near 14% snaps to 125%.
        grid.frame(&click(429.0));
        assert_eq!(grid.menu.entity_distance_percent, 125);
        grid.sliders[0].1 = 0.0;
        grid.frame(&key(KeyCode::ArrowLeft));
        assert_eq!(grid.menu.entity_distance_percent, 50);
        grid.sliders[0].1 = 1.0;
        grid.frame(&key(KeyCode::ArrowRight));
        assert_eq!(grid.menu.entity_distance_percent, 500);
    }

    #[test]
    fn discrete_slider_steps_one_value_per_arrow() {
        let mut grid = Grid::with_slider("First", "FOV: 70", "FOV:");

        grid.frame(&MenuInput::default());
        grid.frame(&tab());
        grid.frame(&tab());
        grid.frame(&key(KeyCode::ArrowRight));
        assert_eq!(grid.menu.fov, 71);
    }

    #[test]
    fn slider_click_then_tab_moves_to_the_next_widget() {
        let mut grid = Grid::with_slider("Sensitivity: 50%", "Second", "Sensitivity:");

        grid.frame(&click(300.0));
        grid.frame(&MenuInput::default());
        grid.frame(&tab());
        assert_eq!(grid.menu.focus, Some(1));
    }

    #[test]
    fn press_and_release_in_one_frame_releases_the_slider() {
        let mut grid = Grid::with_slider("First", "Sensitivity: 50%", "Sensitivity:");
        let tap = MenuInput {
            mouse_held: false,
            ..click(450.0)
        };

        let result = grid.frame(&tap);
        assert!(result.clicked_button);
        assert_eq!(grid.menu.active_slider, None);
        assert_eq!(grid.menu.focus, Some(1));
    }

    #[test]
    fn navigating_away_leaves_the_next_screen_unfocused() {
        let mut grid = Grid::new("First", "Second");
        grid.nav = vec![("First", Screen::OptionsVideo)];

        grid.frame(&click(300.0));
        assert!(matches!(grid.menu.screen, Screen::OptionsVideo));
        assert_eq!(grid.menu.focus, None);
    }

    #[test]
    fn button_click_takes_focus_through_the_shared_helper() {
        let mut ctx = FocusCtx {
            next_index: 0,
            focus: None,
            clicked: true,
            screen_gen: 0,
            activate: false,
            fired: false,
        };
        let mut elements = Vec::new();
        let mut any_hovered = false;
        let fired = push_button_f(
            &mut elements,
            &mut ctx,
            &mut any_hovered,
            (10.0, 10.0),
            true,
            0.0,
            0.0,
            100.0,
            20.0,
            1.0,
            "Go",
            true,
        );
        assert!(fired);
        assert_eq!(ctx.focus, Some(0));
    }

    /// Drives every options screen so `build_options_grid`'s debug assertion
    /// checks each screen's rows against its disabled list.
    #[test]
    fn resource_pack_rescan_keeps_local_order_below_server_priority() {
        let root = std::env::temp_dir().join(format!("pomme-pack-ui-{}", uuid::Uuid::new_v4()));
        for name in ["low", "high"] {
            let pack = root.join("resourcepacks").join(name);
            std::fs::create_dir_all(&pack).unwrap();
            std::fs::write(
                pack.join("pack.mcmeta"),
                br#"{"pack":{"pack_format":88,"description":"test"}}"#,
            )
            .unwrap();
        }
        let mut menu = test_menu(root.to_str().unwrap());
        menu.resource_pack_selection = vec!["low".into(), "high".into()];
        menu.active_packs.push(crate::resource_pack::PackInfo {
            name: "server".into(),
            description: "server".into(),
            compat: crate::resource_pack::PackCompat::Compatible,
            source: crate::resource_pack::PackSource::Server,
            enabled: true,
        });
        menu.rescan_packs = true;
        let text_width = |_: &str, _: f32| 0.0;
        menu.build_options_resource_packs(800.0, 600.0, &MenuInput::default(), &text_width);
        assert_eq!(
            menu.active_packs
                .iter()
                .map(|pack| (
                    pack.name.as_str(),
                    pack.source == crate::resource_pack::PackSource::Server
                ))
                .collect::<Vec<_>>(),
            [("low", false), ("high", false), ("server", true)]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabled_prefixes_cover_every_options_screen() {
        let mut menu = test_menu("pomme-options-coverage-test");
        let input = MenuInput::default();
        let text_width = |_: &str, _: f32| 0.0;
        let tw: common::TextWidthFn = &text_width;
        let (sw, sh) = (1920.0, 1080.0);

        menu.build_options(sw, sh, &input, tw);
        menu.build_options_video(sw, sh, &input, tw);
        menu.build_options_controls(sw, sh, &input, tw);
        menu.build_options_chat(sw, sh, &input, tw);
        menu.build_options_accessibility(sw, sh, &input, tw);
        menu.build_options_music(sw, sh, &input, tw);
        menu.build_options_skin(sw, sh, &input, tw);
        menu.build_options_online(sw, sh, &input, tw);
    }
}
