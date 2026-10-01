# Movement observation capture (2026-10-01)

診断のみ。移動計算、衝突解決、送信選択、brand、packet allowlist は変更しない。
ベースは Client `0a1aee9c6be911a3081cf454b0bae6d694ec9d55`。push は行わない。

## 次の capture

1. 旧プロセスを通常終了して、今回ビルドした `target/dev-fast/pomme-client.exe` を起動する。
2. 接続後、既存の pause → Benchmark の movement recording 開始操作を使い、再現する。
3. recording 停止、または切断後の writer drain 完了まで待つ。画面に出る保存先の
   `movement-*.jsonl` を提供する。`Recording FAILED`、footer の `dropped`、
   `oversize_omitted`、`complete` を必ず確認する。
4. header の `executable` を下記 fingerprint と比較する。path/mtime だけでなく SHA-256 を使う。
   hash は writer の blocking worker で 64 KiB ずつ読む。ゲーム/UI thread では読まない。
   header の `source_build_revision` は未取得なので null。実行時 git HEAD で代用しない。
5. correction の `before_apply` / `applied` と直後の `movement_tick` を seq/offset_us 順に比較する。
   UTC/Instant、schema=1、既存 field、footer の意味は維持される。

## 読む field

- `pose`、`main_supporting_block_pos` は記録時点の状態。
- `travel_observation` は直近の実 travel からコピーした値。correction 自体が再計算した値ではない。
  `used_friction_f32` と `friction_source_pos` は同じ実 sampling の結果。
  `used_ground_drag_f32` は land travel が実際に掛けた drag（空中なら air drag）。
- `used_block_speed_factor_f32`、`used_block_jump_factor_f32`、`jump_power_f32`、
  `used_step_height`、`pose_at_move`、`support_before/after`、`bbox_before/after`。
  jump power は boost を含む計算結果。既存 vy が大きければ max によりその値のままになる。
  各経路で未使用の値は null。騎乗中の travel_observation は null。
- `requested_delta` / `clipped_delta` は既存 collision_delta を再利用。
  `original_requested_y_negative` と `final_y_clipped`、`ground_decision` を比較する。
  最終 ground は元の requested Y < 0 と最終 Y clipping で判断される。step eligibility と混同しない。
- `context` は実 collision 呼び出し時の descending / leather boots / fall distance / entity count / border。
  実 entity collider の AABB は recording 中だけ最大8個コピーし、omitted/truncated を出す。
- 既存 `floor_state` / `block_friction_f32` は prev-position の中心 fallback。
  `legacy_floor_semantics` の通り、実際に使った friction の証拠にはしない。

## 形状 snapshot の限界

clipping のあった tick と correction 前後だけ、既存 collision shape visitor を使って
現在 world の state ID/name/properties と world-space AABB を再問い合わせする。
clipping 時は保存した move bbox/request と step-height envelope、correction 時は現在 bbox を使う。
実際の resolver が集めたリストそのものではないため、`semantics` は常に NOT actual collision inputs。
補正 snapshot の crouching は現在状態であり、実 move の descending input は travel context を見る。

最大32 cell（air を含む、Y/Z/X順）・128 box。`truncated`、`omitted_block_cells`、
`omitted_boxes_in_visited_cells` を見る。未訪問 cell の box 数は不明なので null。
step の候補別リスト、実 border wall AABB、moving-piston raw NBT は保存しない。
entity shape は travel 入力の最大8個のみで、correction 時の entity 再問い合わせは行わない。
per-tick history、非 recording 中の JSON/shape snapshot allocation は追加しない。
chat/token/pack URL/raw packet/raw NBT は記録しない。

## ビルド fingerprint / 検証

通常 `mise run build`、default features / locked / dev-fast、成功。
ファイル: `C:/Users/yuzum/Desktop/mine_rust/Client/target/dev-fast/pomme-client.exe`

- size: 110216704 bytes
- mtime UTC: 2026-10-01T09:40:57.578Z (unix_ms=1790847657578)
- SHA-256: `f25afbabf860ff5517a0b7c37c31c4264431fe588dca03cb53911e4a708c2ee7`

これは新しく生成したファイルの識別であり、起動済み旧 process や source revision の証明ではない。
再ビルドすれば fingerprint は変わり得る。

`mise run test`: client 1220 / protocol 52 / singleplayer 1 passed、0 failed。
singleplayer の既存 terrain-write test 1件は元から ignored。追加 skip はなし。
fmt check / git diff --check 成功。警告は残る。fixture SHA-256、inactive closure 非評価、
edge ice の実 support friction、honey/boost jump、32/128 bounds、8 entity bounds、
query/recording on/off の数値不変をテストした。

## 未確定の根因

提供済み c5d 調査では correction 5〜8 後の requested/clipped Y と ground 判定、
client ascending と server Vy の差が観測されている。main-foot shape / step / support 不一致は候補で、
本変更は原因を確定しない。旧 log の実行 binary が最新だったことも遡って証明できない。
Movement echo false/false、通常 input/sprint/send 選択の問題という断定はしない。
SetEntityData 127 failure と boolean 8 warning は今回 scope 外で、未解明のまま・mask 変更なし。
mod chat 通知→StartConfiguration を true Disconnect と読み替えない。
