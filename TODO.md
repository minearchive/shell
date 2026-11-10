# TODO

## AntiWebUI 移行（branch `spike/antiwebui`）
AntiWebUI（bea4dev/AntiWebUI）は rev 固定の git 依存。描画の差し替えではなく、UI 層の書き直し。

### Phase 0: スパイク（`spike/` クレート、使い捨て）
- [x] workspace に `spike` を足し、`anti-webui`（feature `wayland-shell`）を rev 固定で入れる
- [x] devShell でビルドが通る（wgpu 30 / Vulkan ローダ、edition 2024 に必要な rustc）
  - flake の `LD_LIBRARY_PATH` に `vulkan-loader` を追加した。ないと `Failed to create surface for any enabled backend: {}` になる
- [x] `for_each_output` で出力ごとに `Layer::Top` の面を開く。下と左右に貼り、高さはバー＋余白、`Exclusive::Fixed(48)`
- [x] 余白が透明になる（レイヤーの既定は透明）
- [x] バーとアイランドを、`island_path` を kurbo に移したものを `svg::path(closure)` で描いて一体の形にする
- [x] アイランドの幅と高さを `animated` で動かし、毎フレーム形を変える（クリックで開閉）
- [x] effect から `LayerHandle::set_input_region(Rects)` を呼び、余白をクリックが通り抜ける
- [x] アニメーションの途中でクリックしても反転する（easing で止まって見えただけだった）
- [x] 計測：CPU / メモリ / GPU を今のバーと比べる
  - `spike/measure.sh <プロセス名> [秒]`：CPU、RSS / PSS、DRM fdinfo の GPU 使用率
  - `spike/shmem.sh "$PWD" <バイナリ>`：実際の RAM（i915 の GPU バッファも wl_shm も Shmem に載る）
  - CPU：spike は開閉させ続けても 0.3〜0.4%、render エンジン 0.03%
  - fdinfo の「GPU メモリ 225 MB」は見かけ。i915 はドライバが作ったバッファの大きさを数えていて、裏のページではない
    - 素の wgpu Vulkan デバイス（`spike/src/bin/bare.rs`）だけで 216 MB と出る。アダプタだけなら 0。AntiWebUI のせいではない
    - `MemoryHints::MemoryUsage` にしても、`ANV_PRINTF_BUFFER_SIZE` を変えても同じ
  - 実際の RAM（5 回ずつ）：spike は PSS 30 MB ＋ Shmem 約 40 MiB、my_shell（debug）は RSS 66 MB ＋ Shmem 約 15 MiB
    - 差は 25 MiB ほどで、測定のぶれ（spike 32〜42、my_shell 4〜16）と同程度。誤差の範囲
- [x] 判定：形・入力範囲・計測とも問題なし（メモリの差は測定誤差程度）。Phase 1 に進む

### Phase 1: 状態と入力
- [ ] `UIState` の各フィールドを signal にする：`players`、`warp`、`notifications`、`system`、`player_art`
- [ ] calloop の channel を `ui_channel` に置き換える：Notification / mpris / IPC / KDEConnect / Warp / Action / System / PlayerArt / position
- [ ] `UiEvent` を消す（redraw は不要になる。Dismiss / Clear は signal に書き込む）
- [ ] `CLOCK_TICK` を時計の signal と `set_timeout` に置き換え、通知の期限切れも処理する
- [ ] 単一インスタンスのソケット（`cli.rs`）を、listener スレッドから `ui_channel` へ流す形に残す
- [ ] `config/`（theme.toml）を読む部分は残し、テーマを AntiWebUI のスタイルと MD3E の配色に写す

### Phase 2: コンポーネント
- [ ] `Component` トレイトを廃止し、`fn x() -> impl IntoView` にする
- [ ] バーの並び：`BarSlot` を flex の 左 / 中央 / 右 に置き換える
- [ ] アイランドの調停：各コンポーネントの `IslandRequest` を `Memo` にまとめ、priority 最大を main、他を畳む
- [ ] `IslandMorph` を `animated` に置き換える。中身の伸縮は `scale` で付ける
- [ ] clock
- [ ] media（player_art は `bitmap_view`）
- [ ] notifications（アイランド）
- [ ] toolbox（スライダーは MD3E、notifications のリスト、items）
- [ ] `m3_widget` を `anti-webui-md3e`（独立クレート。同じ git rev から依存に足す）に置き換える。スライダー、アイコン（Material Symbols）、ボタン、リスト項目
  - まず対応表を作る：m3_widget の各部品 → md3e の部品 / 足りないもの
  - テーマ：theme.toml の配色を md3e の配色（ダイナミックカラー）に写す
- [ ] animation.toml の easing と `conf::` 参照：AntiWebUI の `Transition` / `Spring` に写すか、自前の計算を残すか決める

### Phase 3: バナー
- [ ] 出力ごとに `Layer::Overlay` を上端に開き、入力範囲は通知カードの矩形だけにする
- [ ] 上から落ちてくる動きを `animated` で付ける

### Phase 4: 残り
- [ ] DnD：AntiWebUI の OS DnD がレイヤー面で動くか確認し、`dnd.rs` の代わりにする
- [ ] notification_icon（D-Bus の画像データ → bitmap）
- [ ] 出力の抜き差し（`for_each_output` に任せる）を確かめる

### Phase 5: 片付け
- [ ] `ui_core` / `ui_widget` / `m3_widget` / `m3_test` / `macros` を消す（m3_widget を AntiWebUI の MD3E に置き換えると決めた場合）
- [ ] `output.rs` / `seat.rs` / `dnd.rs` / calloop / sctk / skia を依存から外す
- [ ] flake：skia-binaries を外し、vulkan-loader を足す。GTK の残骸も消す
- [ ] CLAUDE.md、docs/widgets.md、docs/components.md を書き直す

## Rendering
- [ ] Optimized rendering

## Fonts
- [ ] Font fallback: Roboto as primary + Noto Sans CJK JP as fallback for CJK glyphs (currently `FontBook` only holds one typeface per key; needs per-char `unichar_to_glyph` check + `match_family_style_character`, with the fallback typeface resolution cached, not called per-draw)
- [ ] From file

## Audio
- [x] status provider
- [ ] setting for each process
- [ ] EQ
- [ ] OSD

## Battery
- [x] status provider
- [ ] alert

## Monitor
- [ ] light level
- [ ] mirroring in niri

## LocalSend & Quickshare
- [ ] support
- [ ] custom widget
- [ ] drag and drop

## IPC
- [ ] mapping all niri events
- [ ] mapping all hyprland events
- [ ] IPC events

## DBUS
- [ ] kde-connect
  - [x] Handling Connect/Disconnect
  - [ ] Notification mapping, tagging it from which device
- [x] notification
- [x] cloudflare-warp 
- [ ] network stats, event

## Monitor
- [ ] status provider
- [ ] mirroring in niri
- [ ] system resources

## Shell
- [x] set keyboard interactivity to None at creation, drop the pointer Enter hack
- [ ] keyboard input, only if the bar ever needs it (needs get_keyboard_with_repeat)
- [ ] make design.
- [ ] z-order based rendering

## Component
- [ ] Material3 Main Component
  - [x] Button
    - [x] Normal
    - [x] Icon
    - [x] Radio
    - [x] Segmented
  - [x] Slider
  - [x] CheckBox
  - [x] Divider
  - [x] ListItem
  - [ ] Textbox
    - [x] keyboard input, cursor, paste
    - [x] selection range
    - [x] copy/cut
    - [ ] IME / preedit, adapters only since Commit is the one insertion path
  - [ ] DatePicker
  - [x] Chips
    - [x] Assist
    - [x] Filter
    - [ ] Input
    - [ ] Suggestion
  - [ ] Navigation
    - [ ] drawer
    - [ ] rail
- [ ] General Positioning Component
  - [x] Scrollable
  - [x] Row
  - [x] Column
  - [ ] Stack
  - [x] Align

# Setting GUI
- [ ] Itegrate with main
  - [ ] realtime mapping with config
  - [ ] event separate from bar
