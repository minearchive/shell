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
  - `scripts/measure.sh <プロセス名> [秒]`：CPU、RSS / PSS、DRM fdinfo の GPU 使用率
  - `scripts/shmem.sh "$PWD" <バイナリ>`：実際の RAM（i915 の GPU バッファも wl_shm も Shmem に載る）
  - CPU：spike は開閉させ続けても 0.3〜0.4%、render エンジン 0.03%
  - fdinfo の「GPU メモリ 225 MB」は見かけ。i915 はドライバが作ったバッファの大きさを数えていて、裏のページではない
    - 素の wgpu Vulkan デバイス（`spike/src/bin/bare.rs`）だけで 216 MB と出る。アダプタだけなら 0。AntiWebUI のせいではない
    - `MemoryHints::MemoryUsage` にしても、`ANV_PRINTF_BUFFER_SIZE` を変えても同じ
  - 実際の RAM（5 回ずつ）：spike は PSS 30 MB ＋ Shmem 約 40 MiB、my_shell（debug）は RSS 66 MB ＋ Shmem 約 15 MiB
    - 差は 25 MiB ほどで、測定のぶれ（spike 32〜42、my_shell 4〜16）と同程度。誤差の範囲
- [x] 判定：形・入力範囲・計測とも問題なし（メモリの差は測定誤差程度）。Phase 1 に進む

### Phase 1: 状態と入力
- [x] `UIState` を `src/state.rs` の `State`（signal の束）にする：`players`、`player_art`、`last_playing`、`warp`、`notifications`、`system`、`focused_title`、`focused_workspace`、`theme`、`animation`、`action`
  - コマンドの送り口と `NotificationHandle` は `State::commands`（`Rc<Commands>`）
- [x] calloop の channel を `ui_channel` に置き換える。プロバイダは `UiSender` で送る。`cli` と config の監視はコールバックを受け取る形にした（テストは std の mpsc）
- [x] `UiEvent` を消す：Dismiss / Clear は `State::dismiss_notification` / `clear_notifications`、設定の再読み込みは signal の更新
- [x] `CLOCK_TICK`：期限切れの確認は毎秒の tick スレッド＋`ui_channel`（`set_timeout` はウィンドウごとのタイマーで、Shell のルートでは使えない）。時計は `local_time(Every::Minute)`
- [x] 単一インスタンスのソケットはそのまま。`action` は連番付きの signal（同じ action が 2 回来ても読む側が走り直す）
- [x] 仮のバー（`main.rs` の `bar`）に全プロバイダの値が出ることを確認した。Phase 2 で消す
- [ ] テーマを MD3E の配色に写す → Phase 2（md3e の対応表と一緒に）
- 気付いたこと
  - `example/theme.toml` は .gitignore 対象で worktree に来ない。ないと既定の `Theme` が全部透明で、何も見えない。既定値を見える色にするか、ないときに警告以上を出す
  - niri の IPC は変化のときしかイベントが来ないので、起動直後は ws / タイトルが空。最初に `Request::Workspaces` / `FocusedWindow` を引く
  - 起動時に Vulkan ローダの ERROR が出る：システムの Mesa の radeon / lvp の ICD が、devShell の glibc より新しい glibc を要求して読み込めない。Intel の ICD は読めているので描画には影響しない
  - [x] バーの文字がマウスで選択できてしまう → バー全体に `select_none`

### Phase 2: コンポーネント
- [x] `Component` トレイトを廃止し、コンポーネントは `Part` を返す関数にする（`src/ui.rs`）：バーに置く View と、アイランドの入札（`Memo<Option<IslandRequest>>`）＋中身の View
- [x] バーの並び：`BarSlot` は flex の 左 / 中央 / 右。幅の計算（`bar_width` / `bar_origins`）は消えた
- [x] アイランドの調停：入札を effect で読み、priority 最大（同点は後の方）を main、他を畳む
- [x] モーフは `IslandMorph` をそのまま使い、動いている間だけ `request_frame` で `frame` signal を進める。`animation.toml` の `island` の easing もそのまま効く
  - シルエットは `svg::path`、中身は `translate` / `scale` / `opacity`、入力範囲は `frame` を読む effect から
- [x] `draw_overlay` の代わり：`Part::overlay`（バーとアイランドの上に、パートのバーでの位置から描く）と `IslandPart::frame`（`bar` が毎フレーム書く `IslandFrame`：開き具合と、開き切ったときの中身の位置）
  - パートの x はバーの並びから計算する（`part_x`。AntiWebUI には要素の位置を聞く API がない）
  - toolbox：バーのアイコンがパネルの各項目へ飛ぶ（電池はフェード）、通知パネルではその場でフェード。閉じるアイコンと未読バッジは開き具合でクロスフェード
  - media：タイトルと各行がバーの文字の位置からカードへ移る（`travel`）、アートはサムネイルからカードへ広がりつつ半分で消える、アートなしは音符が再生ボタンへ
  - [ ] 実機で確認：位置のずれ（バーの文字の行の高さは AntiWebUI の既定 1.4 と仮定）
- [x] `captures_outside_press`（`catcher.rs`）の代わり：アイランドが開いている間だけ、全画面の透明な `Layer::Top` を開き、バーとアイランドの矩形を入力範囲から抜く（`ui::cut_out`）。押されたら全部の `IslandPart::close` を呼ぶ
  - レイヤーは出力ごとに 1 つ開いたまま、閉じている間は 1×1 で入力なしに縮める。開くたびに作ると、AntiWebUI はウィンドウごとに wgpu のインスタンスとデバイスを作るので、開く瞬間にラグが出た
  - [ ] 実機で確認：外クリックで media / toolbox が閉じる、バーとアイランドはそのまま押せる、閉じた後に画面のクリックが通る
  - バーを押すと、押したパート以外のアイランドが閉じる（パートの外側の div の `on_pointer_down`、パート自身の処理の後）。バーの空いた所は全部閉じる。これがないと、toolbox を開いたまま media を押しても priority で負けて出ない
- [x] 開閉は押した瞬間（`on_pointer_down`）。`on_click` は離したときで、押している間のぶん遅れていた
  - 外クリックのレイヤーの 1×1 ⇄ 全画面のサイズ変更は、同じスレッドのフレームを止めないことを計測済み（spike で毎秒切り替え、最長の間隔は 18〜20ms のまま）
- [x] 確認用の `debug_island` を消した
- [x] clock
- [x] media
  - カードの背景はアートの `bg_image`（`BackgroundSize::Cover`）、スクリムは svg の角丸矩形に線形グラデーション
  - `Art` は skia の `Image` ではなく `ImageSource`（RGBA）を持つ。デコード・縮小・アクセント色は今まで通り skia で、プロバイダのスレッドで
  - シークバーは `slider_with`（挙動だけ）に自前の見た目。md3e の `wavy` は色が全体のスキーム固定で、波もホバー中しか流れないため使わない。アートの色と「再生中に流れる波」は今まで通り
  - 曲名の折り返しと省略は `ui_core::util` のまま、文字幅は `svg::measure_text` で測る
  - [x] 再生ボタンの角の変形は `animation.toml` の `media_play_pause` で動く（`ui::eased`：ui_core の `Animation` を `request_frame` で進める）
  - [x] 波と再生位置の確認：Firefox（長さを報告する）の再生中のカードで、10:02 / 12:55 と波が出ているのをスクリーンショットで確認
- [x] notifications：`rewrite` でアイランドから上端のバナーになっていたので、Phase 3 を先にやった（下）
- [ ] toolbox
  - [x] バーのアイコン列（Material Symbols）、未読バッジ、左クリックでコントロール、開いている間は閉じるアイコン
  - [x] コントロールのパネル：`panel_layout` はそのまま、各項目を絶対配置。タイル、WARP は md3e の switch、音量と明るさは md3e の slider、電源プロファイルは md3e の connected button group
    - スライダーは、動かしてから 600ms はポーリングの値を無視する（ドラッグの終わりを slider が教えてくれないため）
    - 電源プロファイルの「使えない項目」は押しても何も送らないだけで、見た目は無効にならない
  - [x] 右クリックの notifications のリスト：アプリごとに重ねる、クリックで開く、ホバーで ✕、すべて消去、`overflow_y_scroll` でスクロール、毎分「n分前」を更新
  - [x] マウスで横にドラッグして消す（`on_grab`、最初の動きが横のときだけ。幅の 4 割を超えて離すと滑って消え、超えなければ戻る）。✕ も滑って消える
    - タッチパッドの横スクロールで消すのは入れない（Skia 版で誤作動が多かった）
  - [x] 行のアニメーション：`for_each` をキー（先頭の id と種類）で。消えた行の詰めとアプリの開閉は `moves`、出てくる行は上から少し下りつつフェード、消える行はフェード。すべて消去は上から 50ms ずつずらして滑る（4 行目以降は一緒）
    - 行の中身は `rows` の memo から読むので、毎分の「n分前」の更新で行が作り直されない
  - [ ] 実機で確認：ドラッグで消える／戻る、✕、すべて消去、アプリの開閉の動き
- [x] 反応の遅さ：音量は `wpctl` の 1 秒ポーリングをやめ、AntiWebUI の `shell-audio`（PipeWire のイベント）から。電源プロファイルと WARP は押した値をすぐ出し、状態が変わるか 3 秒で実際の値に戻る
  - 計測：描画の経路（通知 → バナー）は 50ms 以内。遅れはプロバイダのポーリングだった（ミュート切替 → アイコンが 315〜573ms）
  - [ ] 実機で確認：音量キー・明るさキーでスライダーとアイコンがすぐ追従するか
  - [x] 明るさも `shell-session` のバックライト監視（`actual_brightness` の inotify）から。1 秒のポーリングはなくなった（このマシンで書き込み直後にイベントが来るのを確認済み）
- [x] theme.toml の配色を md3e の `set_color_scheme` に写す（`ui::md3e_scheme`、テーマの再読み込みにも追従）
- [ ] `m3_widget` を `anti-webui-md3e`（独立クレート。同じ git rev から依存に足す）に置き換える。スライダー、アイコン（Material Symbols）、ボタン、リスト項目
  - まず対応表を作る：m3_widget の各部品 → md3e の部品 / 足りないもの
  - テーマ：theme.toml の配色を md3e の配色（ダイナミックカラー）に写す
- [ ] animation.toml の easing と `conf::` 参照：AntiWebUI の `Transition` / `Spring` に写すか、自前の計算を残すか決める

### Phase 3: バナー
- [x] 出力ごとに `Layer::Overlay` を上端に開き、入力範囲は通知カードの矩形だけにする
- [x] 上から落ちてくる動き：`animated` ではなく `IslandMorph` のまま（`notification_banner` の easing が効く）
- [x] 通知のアイコンは D-Bus のスレッドで `ImageSource` にする（`player_art::pixels`）
- [ ] 実機で確認：`notify-send` で落ちてくる、クリックで消える、`+n`、critical の色

### Phase 4: 残り
- [x] DnD：`dnd.rs` を消し、バーの root の `on_drop_files` / `on_drop_text` にした。Skia 版と同じくログに出すだけ（受け取るパートはまだない）
  - AntiWebUI の `wl_data_device` は面を区別しないので、レイヤー面にも届くはず
  - [ ] 実機で確認：ファイルマネージャからバーにファイルを落とすと `RUST_LOG=info` で `Dropped files` が出る
- [x] notification_icon：Phase 3 でやった（`player_art::pixels` で `ImageSource`）
- [ ] 出力の抜き差し（`for_each_output` に任せる）を確かめる

### Phase 5: 片付け
- [x] `ui_widget` / `m3_widget` / `m3_test` / `macros` と `spike` クレートを消した（計測スクリプトは `scripts/` に残した）
  - `ui_core` は残し、skia を使う所（`island_path`、`FontBook`、`BoundingBox`、`to_skia`、`Color4f`）と `pointer` / `keyboard` を消した。残りは幾何、配色、easing、`IslandMorph`、`elide` / `wrap_text`
  - skia の `island_path` のテストは `ui.rs` の kurbo 版に移した
- [x] `output.rs` / `seat.rs` / `dnd.rs` を消し、skia を依存から外した（calloop / sctk はもう入っていなかった）
  - 画像のデコード・縮小・アクセント色・通知アイコンの角丸は `image` クレート（AntiWebUI と同じ 0.25 なので、ビルドに増えるものはない）
- [x] flake：skia-binaries と GTK、clang / cmake / ninja を外した。リンクに要るのは dbus、fontconfig、wayland、libxkbcommon、libxcb（AntiWebUI の X11 バックエンドが常に入るため）
  - パッケージの `wrapProgram` に、devShell と同じ `LD_LIBRARY_PATH`（vulkan-loader、mesa、libGL、wayland、libxkbcommon、pipewire）を足した。`nix build` が通ることを確認済み
  - AntiWebUI の git 依存の `outputHashes` を足した。rev を上げたら更新する
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
