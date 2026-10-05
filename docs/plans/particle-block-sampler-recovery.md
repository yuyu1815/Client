# `blocks.rs` block sampler recovery

## 結果

`pomme-client/src/world/particle_tick/blocks.rs` の production sampler を復旧した。復旧後のスナップショット:

- 1,990 行 / 63,928 bytes
- SHA-256: `83ff1b53697cec2f6f44280e2990455fbb382e64701425086270a1fc038f13c1`
- 作業時点: 2026-10-05 (local workspace)
- Git status: `pomme-client/src/world/particle_tick/blocks.rs` と本記録が untracked (`??`); commit なし

破壊前に確認された長さは 1,813 行 / 58,087 bytes。最初の再構成時点では 1,940 行で、破壊前より 127 行多かった。現在の最終ファイルは 1,990 行 / 63,928 bytes であり、破壊前より **177 行 / 5,841 bytes** 多く、最初の再構成からは 50 行増えている。これらは異なる観測時点の行数・byte 数であり、元の全バイトを復元したものではない。複数時点の transcript read をコード内容のアンカーと時系列で照合して再構成した。復旧後は **stub 化や sampler 機能の削除をしていない**。現行ファイルは確認済み source の関数群・Java 原本から再生成した表である。

以前記録された「134 行未カバー」は、その当時の transcript read coverage（読めていなかった範囲）の数であり、最終 source 行数との差、全行の同一性確認、または復旧できた行数を意味しない。元スナップショットは byte-for-byte 比較可能な形で保持されておらず、byte-for-byte 復元・同一性の証明は不可能だった。この限界は維持する。

## 回収・再構成の内訳

### Transcript から回収した内容

- `sample_positions`: `ecb1d431-1b5f-460.output` の 1–320 / 430–495、`4714a04c-e051-4b7.output` の中間ブロック、`d5afb294-ed11-4b9.output` の末尾を、分岐・式の内容アンカーで接続した。
- 完全な関数 body を含む read 群から、醸造台・portal・end gateway・pointed dripstone・lighting rod・candle・leaves・redstone・drip・shape/tag 等の production helpers を回収した。
- `wire_line`, `drip_position`, `impermeable` 等も complete body の read 結果から復元した。`wet_sponge` は read の実装形を Java `WetSpongeBlock.animateTick` と照合し、6方向から一つ選択して上方向だけ早期 return、`canOcclude` + 反対面 sturdy 判定に合わせた。
- 破壊直前の sulfur test は `blocks_recovery-sulfur-tests.rs` に退避して保持し、real `BlockState` fixture を再び production sampler に通す形で統合した。

### Java 原本を使って再構成した内容

- `JAVA_26_2_FLAMMABLE_BLOCKS`: Java 26.2 `FireBlock.java` の `setFlammable(Blocks..., ...)` 登録から生成。件数 175、重複なし、全 ID が native block registry に存在することをテストする。
- `crying_obsidian`: transcript に関数全体がなく、`CryingObsidianBlock.animateTick` (`minecraft-26.2-decompiled/src/net/minecraft/world/level/block/CryingObsidianBlock.java:61–73`) に合わせ、方向を 1 つ選択、上方向は除外、source の `canOcclude` と反対面の sturdy 判定、1 個の `DRIPPING_OBSIDIAN_TEAR` を再構成した。
- Wither Rose の欠けた座標は `WitherRoseBlock.animateTick` (`.../WitherRoseBlock.java:61–73`) の smoke の位置式に合わせた。
- 下側に対する `BaseFireBlock` 分岐は依頼どおり `can_burn(below) || sturdy_top(below)` とした。実ブロック状態テストで flammable oak planks と sturdy stone の双方を確認する。

## 保持・接続した条件

- `sample_positions` は caller から渡される共通 animate probe を処理するだけで、別の 667×2 probe stream を再抽選しない。接続は `world/particle_tick.rs::sample_block_particles_at_positions` → `blocks::sample_positions` (`particle_tick.rs:241–255`)。
- Pointed dripstone は位置ごとの `dripstone_attributes` callback から `WATER_EVAPORATES` と typed `DEFAULT_DRIPSTONE_PARTICLE` を取得する。
- `potent_sulfur` は実 state property `potent_sulfur_state` の `dry` 判定と、上側の source water 条件を使い、2 個の sulfur bubble を生成する。BE の timer/geyser/gas 経路とは別の block animate-tick source。
- `IMPERMEABLE` は未受信 (`None`) 時の vanilla fallback、明示的な空集合 (`Some(empty)`) と server override を区別する。
- Leaves は `MotionBlockingNoLeaves` heightmap を使い、redstone ore は `is_solid_render` 判定、drip/honey は collision shape・heightmap・position を使う。

## 確認結果

Client repository root (`Client`) から、直列で実行した。

| Command | Result |
|---|---|
| `mise exec -- rustfmt --edition 2024 pomme-client/src/world/particle_tick/blocks.rs` | exit 0 |
| `mise exec -- rustfmt --edition 2024 --check pomme-client/src/world/particle_tick/blocks.rs` | exit 0 |
| `mise run check` | exit 0。`pomme-client` を含む dev-fast check 成功。既存 warning あり。 |
| `mise exec -- cargo test -p pomme-client --locked world::particle_tick::blocks::tests --no-fail-fast` | Recovery-stage result: exit 0, 5 passed / 0 failed / 1,746 filtered. Final snapshot adds `lit_redstone_ore_animate_tick_covers_stone_and_deepslate_variants`; the block test group is now 6 passed (see integration snapshot). |

Recovery-stage focused tests: `java_class_identity_candles_and_fire_registry_are_exact`, `five_block_sampler_inputs_produce_java_boundary_results`, `base_fire_uses_real_flammable_and_sturdy_block_states`, `drip_collision_shapes_and_impermeable_tag_are_respected`, `potent_sulfur_animate_tick_uses_real_state_property_and_source_water_only`。Final snapshot adds `lit_redstone_ore_animate_tick_covers_stone_and_deepslate_variants` (six block tests total).

なお、復旧途中の `cargo check` / focused test は、同時編集されていた `particle.rs` の `Kind` move エラーと、一時的な test assembly 不備で失敗した。これらは最後の serial check/test では解消しており、最後の結果は上表。

`git diff --check` は exit 0。untracked source 自体は通常の `git diff --check` 対象外のため、別途 trailing-whitespace scan を実施し、該当行なし (`rg` exit 1) を確認した。

## 制限と最新確認

元の 1,813 行 snapshot の byte-for-byte 復元ではない。Java 原本で再構成した範囲は上に明記した。最新 workspace suite は `particle-integration-verification.md` を参照（client 1,766 / protocol 52 pass）。ゲーム内の視覚 A/B と元 particle screenshot の再現は未実施。
