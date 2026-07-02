# KDE Connect 実装プラン

対象機能: **Battery 表示 / Share / Notification / SMS / Mpris(端末側) / Connect・Disconnect ハンドリング**

このドキュメントは、既存アーキテクチャ（`src/dbus/mpris.rs` のスレッドモデル、`calloop::channel` の上り配線、`Component` trait、`UIState`）に沿って KDE Connect を実装するための設計方針をまとめたもの。traitレイヤ（`src/dbus/kdeconnect.rs`）は既に実装済みなので、その上に載せる「クライアント／状態／描画／操作」の層を定義する。

---

## 0. 現状整理

すでに存在するもの:

- `src/dbus/kdeconnect.rs`: `Daemon` + 各プラグインの zbus proxy trait（`Device`, `Battery`, `Notifications`, `Notification`, `Sms`, `Telephony`, `MprisRemote`, `Share`, `Conversations` など）。
- `KDEConnectEvent` enum（暫定・粗い）と、空実装の `KdeConnectClient::init`。
- `src/main.rs`: `kde_channel` は既に配線済み。`KdeConnectClient::init(kde_tx)` を呼び、`Msg` を `screen.ui.on_kde_connect_event(&ev)` にファンアウトしている。
- `src/ui.rs`: `Component::on_kde_connect_event` と `UserInterface::on_kde_connect_event` は存在するが、状態更新なし（componentsに配るだけ）。

つまり **上り（D-Bus → UI）の骨格は通っている**。埋めるべきは (1) クライアント実装、(2) イベント設計、(3) UIState拡張、(4) 描画コンポーネント、(5) 下り（UI → D-Bus 操作）チャンネル。

---

## 1. 全体アーキテクチャ

```
                    ┌──────────────────────────── background thread (tokio current-thread rt) ──┐
                    │  KdeConnectClient                                                          │
                    │   ├─ DaemonProxy         (deviceAdded/Removed, reachableChanged)           │
   D-Bus (session)  │   ├─ per-device proxies  (Battery / Notifications / MprisRemote / …)       │
   org.kde.kdeconnect│   └─ StreamExt::merge した signal stream を1タスクで select!             │
                    └───────────┬───────────────────────────────────▲───────────────────────────┘
                                │ KDEConnectEvent (上り)              │ KdeConnectCommand (下り)
                    calloop::channel::Sender                std::sync::mpsc / tokio::mpsc
                                │                                    │
              ┌─────────────────▼────────────────┐                  │
   main.rs    │ kde_channel → ui.on_kde_connect  │                  │
              └─────────────────┬────────────────┘                  │
                                │                                    │
   ui.rs      UIState.kde を更新 ──► RequestRedraw ──► draw          │
              components (Battery/Notif/MediaRemote) が UIState を読んで描画        │
              on_cursor でボタン押下 ──────────────► cmd_tx.send(KdeConnectCommand)┘
```

**2本のチャンネル**を張るのが肝:

1. **上り** `calloop::channel::Sender<KDEConnectEvent>`（既存）: D-Bus signal/property を UI に届ける。
2. **下り** `Sender<KdeConnectCommand>`（新規）: UI からの操作（ring, share, mpris control, notification reply/dismiss）をバックグラウンドスレッドに渡し、そこで async のメソッド呼び出しを実行する。proxy はバックグラウンドスレッド側に住むので、UI スレッドから直接呼べないため必須。

---

## 2. 非同期ランタイムの選択

`mpris.rs` は同期クレート + `thread::spawn` + ループ。zbus は async ファースト。方針:

- **`KdeConnectClient::init` で `thread::spawn` を1本立て、その中で current-thread tokio ランタイムを `block_on` する**（`mpris.rs` のスレッドモデルを踏襲しつつ中身は async）。
- `Cargo.toml` の zbus に `features = ["tokio"]` を追加し、`tokio` は `features = ["rt", "macros", "sync"]` を明示（現在は default のみ）。
- 複数の signal stream（daemon + デバイスごとに複数プラグイン）を **1タスクで捌く**。async なら `futures::stream::select_all` / `StreamExt::merge` + `select!` でまとめられる。blocking API だとストリームごとにスレッドが要り管理が煩雑なので **async 版を採用**。

```rust
pub fn init(sender: Sender<KDEConnectEvent>) -> Sender<KdeConnectCommand> {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<KdeConnectCommand>();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all().build().expect("rt");
        rt.block_on(async move {
            if let Err(e) = run(sender, cmd_rx).await {
                error!("kdeconnect client stopped: {e}");
            }
        });
    });
    cmd_tx
}
```

`main.rs` 側は戻り値の `cmd_tx` を `Shell`（または各 `UserInterface`）に保持させる。

---

## 3. イベント設計（`KDEConnectEvent` の再設計）

現状の enum は粗い（`String` の羅列でどの信号か曖昧）。**機能単位＋device_id 付き**に作り直す。device_id を常に持たせることで、将来マルチデバイスにも耐える。

```rust
#[derive(Clone, Debug)]
pub enum KDEConnectEvent {
    // ---- 接続状態 ----
    DeviceAdded    { id: String, name: String },
    DeviceRemoved  { id: String },
    Reachability   { id: String, reachable: bool },       // reachableChanged
    PairState      { id: String, paired: bool },          // pairStateChanged

    // ---- Battery ----
    Battery        { id: String, charge: i32, charging: bool },

    // ---- Notification ----
    NotificationPosted  { id: String, notif: NotificationInfo },
    NotificationRemoved { id: String, public_id: String },
    NotificationsCleared { id: String },

    // ---- Telephony / SMS ----
    CallReceived { id: String, event: String, number: String, contact: String },
    // SMS本文はNotification経由で来ることが多いので、SMS専用UIを作る場合のみ Conversation 系を足す

    // ---- Share ----
    ShareReceived { id: String, url: String },

    // ---- Mpris (端末側プレイヤー) ----
    MprisUpdate { id: String, state: MprisRemoteState },  // propertiesChanged をまとめて反映
}

#[derive(Clone, Debug, Default)]
pub struct NotificationInfo {
    pub public_id: String,   // notifications/<id> の <id>
    pub app_name: String,
    pub title: String,
    pub text: String,
    pub ticker: String,
    pub icon_path: String,
    pub dismissable: bool,
    pub reply_id: String,     // 空でなければ返信可能
}

#[derive(Clone, Debug, Default)]
pub struct MprisRemoteState {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub can_seek: bool,
    pub length: i32,
    pub position: i32,
    pub volume: i32,
    pub album_art_url: String,
}
```

`propertiesChanged`（mprisremote）や `refreshed`（battery）は「変更あり」しか通知しないシグナルなので、**シグナル受信をトリガに proxy から現在値を全読みして** `MprisUpdate` / `Battery` を組み立てる。

---

## 4. 下りコマンド設計（`KdeConnectCommand`）

```rust
pub enum KdeConnectCommand {
    Ring          { id: String },
    ShareText     { id: String, text: String },
    ShareUrl      { id: String, url: String },
    // Notification
    DismissNotification { id: String, public_id: String },
    ReplyNotification   { id: String, public_id: String, message: String },
    // Mpris remote
    MprisAction   { id: String, action: String },   // "Play"/"Pause"/"Next"/"Previous"
    MprisSeek     { id: String, offset: i32 },
    MprisSetVolume{ id: String, volume: i32 },
    // SMS
    SendSms       { id: String, addresses: Vec<String>, body: String },
}
```

バックグラウンドの `run` ループで `select!` の一枝として `cmd_rx` を受け、該当 device の proxy を引いて `await` する。proxy は device_id → 各 `*Proxy` の `HashMap` にキャッシュしておく。

---

## 5. `KdeConnectClient::run` の骨格

```rust
async fn run(tx: Sender<KDEConnectEvent>, cmd_rx: Receiver<KdeConnectCommand>) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let daemon = DaemonProxy::new(&conn).await?;

    // 1. 既存デバイスを列挙して初期状態を送る
    let mut devices: HashMap<String, DeviceHandles> = HashMap::new();
    for id in daemon.devices_reachable(true).await? {
        add_device(&conn, &id, &mut devices, &tx).await?;
    }

    // 2. daemon シグナル + 全デバイスのプラグインシグナル + cmd_rx を1ループで select!
    let mut device_added = daemon.receive_device_added().await?;
    let mut device_removed = daemon.receive_device_removed().await?;
    // … 各デバイス subscribe 分を select_all でまとめる（後述）

    loop {
        tokio::select! {
            Some(sig) = device_added.next() => { /* add_device → DeviceAdded 送信 */ }
            Some(sig) = device_removed.next() => { /* DeviceRemoved 送信、devices から除去 */ }
            // per-device signals（動的なので stream を束ねて回す。実装は StreamMap 等）
            Some(cmd) = poll_cmd(&cmd_rx) => { handle_command(&devices, cmd).await; }
        }
    }
}
```

**`add_device`** の中で各プラグインの proxy を生成し（path = `/modules/kdeconnect/devices/<id>[/plugin]`）:

- `BatteryProxy` → 初期 `charge`/`is_charging` を読んで `Battery` 送信、`receive_refreshed()` を購読。
- `NotificationsProxy` → `active_notifications()` を全読みし、各 `NotificationProxy` からプロパティを読んで `NotificationPosted` を投げる。`receive_notification_posted/removed/updated` を購読。
- `MprisRemoteProxy` → 初期値読み → `MprisUpdate`、`receive_properties_changed()` 購読。
- `TelephonyProxy` → `receive_call_received()`。
- `ShareProxy` → `receive_share_received()`。
- `DeviceProxy` → `receive_reachable_changed()`, `receive_on_pair_state_changed()`。

> 動的にデバイスが増減するため、signal stream は固定の `select!` 枝ではなく **`futures::stream::SelectAll` / `tokio_stream::StreamMap`** に device_id をキーで出し入れする形にすると綺麗。各ストリームは `map` で「どの device のどの種類か」のタグ付き enum に正規化してから merge する。

---

## 6. UIState 拡張

`mpris` が `UIState.players: HashMap` を持つのと同じ流儀で、KDE Connect 状態を1つの構造体に集約する。

```rust
// ui.rs
pub struct UIState {
    pub players: HashMap<String, PlayerState>,
    pub workspace_id: String,
    pub window_title: String,
    pub padding: f32,
    pub kde: KdeConnectState,          // 追加
}

#[derive(Default)]
pub struct KdeConnectState {
    pub devices: HashMap<String, DeviceView>,   // 接続中デバイス
}

#[derive(Default)]
pub struct DeviceView {
    pub name: String,
    pub reachable: bool,
    pub paired: bool,
    pub battery: Option<(i32, bool)>,           // (charge, charging)
    pub notifications: Vec<NotificationInfo>,   // public_id で dedup/更新
    pub mpris: Option<MprisRemoteState>,
    pub last_call: Option<(String, String)>,    // (number, contact)
}
```

`UserInterface::on_kde_connect_event` を「componentsに配る」だけでなく **UIState.kde を更新 → RequestRedraw** する形に拡張（`on_mpris` と同じパターン）:

```rust
pub fn on_kde_connect_event(&mut self, event: &KDEConnectEvent) {
    self.components.iter_mut().for_each(|c| c.on_kde_connect_event(event));
    match event {
        KDEConnectEvent::DeviceAdded{id,name} => { self.state.kde.devices.entry(id.clone())
            .or_default().name = name.clone(); }
        KDEConnectEvent::DeviceRemoved{id} => { self.state.kde.devices.remove(id); }
        KDEConnectEvent::Battery{id,charge,charging} => {
            if let Some(d)=self.state.kde.devices.get_mut(id){ d.battery=Some((*charge,*charging)); } }
        KDEConnectEvent::NotificationPosted{id,notif} => { /* upsert by public_id */ }
        KDEConnectEvent::NotificationRemoved{id,public_id} => { /* retain */ }
        KDEConnectEvent::MprisUpdate{id,state} => { /* set */ }
        // …
    }
    let _ = self.sender.send(UiEvent::RequestRedraw(self.idx));
}
```

---

## 7. 描画コンポーネント

`src/components/` に機能ごとに追加。`Clock` と同じく `Component` を実装し、`draw` で `UIState.kde` を読む。各 `mod.rs` に登録し、`UserInterface::new` の `components` vec に push。

| Component | ファイル | 描画 | 操作(on_cursor) |
|---|---|---|---|
| `KdeBattery` | `kde_battery.rs` | 端末バッテリ％＋充電アイコン | なし |
| `KdeNotifications` | `kde_notifications.rs` | 直近通知のタイトル/本文/アプリアイコン | クリックで dismiss / reply |
| `KdeMediaRemote` | `kde_media_remote.rs` | 端末側再生中の曲・アーティスト・再生/一時停止 | ボタンで MprisAction |
| `KdeConnectivity` | (任意) | 接続デバイス名・reachable アイコン | クリックで ring |

- **アイコン**: `NotificationInfo.icon_path` / `MprisRemoteState.album_art_url` は `/tmp/kdeconnect_*` や `file://…` の実ファイル。skia の `Image::from_encoded` で読み込んでキャッシュ（毎フレーム読まない。path をキーに `HashMap<String, Image>`）。
- **操作**: `on_cursor` の `Press` でヒットテスト（コンポーネントが自分の描画矩形を保持）し、`cmd_tx.send(KdeConnectCommand::…)`。`cmd_tx` は各 Component に `new` で渡す（`Clock` が `Sender<UiEvent>` を持つのと同じ流儀）。
- 現在 `ui.rs::draw` は workspace/title をインラインで描いている。KDE Connect も最初はインラインで出して動作確認 → 落ち着いたら Component に切り出す、の順が安全。

---

## 8. 機能別サマリ（実装粒度）

### 8.1 Connect / Disconnect ハンドリング（最優先・土台）
- Daemon `deviceAdded`/`deviceRemoved`、Device `reachableChanged`/`pairStateChanged`。
- 起動時に `devices_reachable(true)` で初期列挙。
- これが動くと「デバイスの出入りに応じて per-device proxy を張り直す」土台ができ、他機能は全部この上に乗る。**まずここから。**

### 8.2 Battery 表示
- `BatteryProxy`: 初期 `charge`/`is_charging` 読み → `receive_refreshed()` 購読。
- 最小・依存少。8.1 の直後に入れて配線を検証する題材に最適。

### 8.3 Notification
- `NotificationsProxy`: `notification_posted/removed/updated/all_notifications_removed`。
- posted/updated 受信 → `NotificationProxy`(path=`…/notifications/<public_id>`) で全プロパティ読み → `NotificationPosted`。
- 操作: `dismiss()` / `send_reply(message)`（`reply_id` が非空のときのみ返信可）。
- アイコンファイル読み込みのキャッシュがここで必要になる。

### 8.4 SMS
- **受信表示だけなら Notification 経由で十分**（SMS はメッセージアプリの通知として飛んでくる）。まずはこれで妥協。
- 本格 SMS UI（会話一覧・送信）を作るなら `Sms`/`Conversations` インターフェース＋`Telephony.callReceived` を使う。会話は request/response が重く、`av`(variant配列)のデコードが必要なので **フェーズ2以降**。送信は `SendSms` コマンドで `send_sms(addresses, body, [])`。

### 8.5 Mpris（端末側プレイヤー）
- **ローカル MPRIS(`src/dbus/mpris.rs`) とは別物**。`org.kde.kdeconnect.device.mprisremote` は「スマホ側」の再生。UI 上は別セクション/別コンポーネントで扱う。
- `MprisRemoteProxy`: `properties_changed` 受信 → 全プロパティ読み → `MprisUpdate`。
- 操作: `send_action("Play"/"Pause"/"Next"/"Previous")`, `seek`, `set_volume`, `set_player`。
- position は自走しないので、必要なら再生中だけUI側でタイマー補間。

### 8.6 Share
- 受信: `share_received(url)` → 通知的に表示（トースト or リスト）。
- 送信: `ShareText`/`ShareUrl` コマンド → `share_text`/`share_url`。クリップボード連携やD&Dは将来。

---

## 9. `main.rs` / 配線の変更点

1. `KdeConnectClient::init` の戻り値を `Sender<KdeConnectCommand>` に変更し、`Shell` に `kde_cmd_tx` を保持。
2. `UserInterface::new` に `kde_cmd_tx: Sender<KdeConnectCommand>` を追加し、各 KDE Connect コンポーネントへ配る。
3. 既存の `kde_channel` 受信ループはそのまま（`on_kde_connect_event` の中身だけ拡張）。
4. `Cargo.toml`: zbus に `features = ["tokio"]`、tokio に `features = ["rt","macros","sync"]`、`futures-util`（or `tokio-stream`）を追加。

---

## 10. 実装マイルストーン（推奨順）

1. **M1 ランタイム土台**: `Cargo.toml` 更新 + `KdeConnectClient::init` の tokio スレッド + `run` の空ループ。`session()` 接続だけ確認（`RUST_LOG=debug`）。
2. **M2 Connect/Disconnect**: daemon 列挙 + add/remove/reachable。`DeviceAdded/Removed` を UIState に反映し、接続デバイス名を（まずインラインで）描画。
3. **M3 Battery**: `Battery` イベント → `DeviceView.battery` → `KdeBattery` コンポーネント。
4. **M4 Mpris remote**: `MprisUpdate` 表示 → `send_action` の下りコマンドで再生/停止まで。下りチャンネル + on_cursor ヒットテストの検証。
5. **M5 Notification**: posted/removed 表示 + アイコンキャッシュ + dismiss/reply。
6. **M6 Share**: 受信表示 + 送信コマンド。
7. **M7 SMS**: まず Notification 経由で受信表示、余力で `Conversations` に着手。

M1→M4 まで通れば「上り・下り両チャンネル + 状態 + 描画 + 操作」の全経路が検証済みになり、残りは各インターフェースを同じ型に流し込む作業になる。

---

## 11. 決めておくべきこと / 注意点

- **マルチデバイス方針**: 当面「reachable な最初の1台」を主表示にするか、全デバイスを並べるか。イベントは device_id 付きにしておくのでデータ構造は両対応。UI の初期実装は「主デバイス1台」で始めるのが簡単。
- **`av`/`a{sv}` のデコード**: Conversations/SMS 系は variant 配列。着手は後回し。トレイトでは `Vec<OwnedValue>` で受けてあるので、必要時にパーサを書く。
- **シグナル駆動の値更新**: `refreshed`/`propertiesChanged` は差分を運ばない。必ず proxy 再読み込みで現在値を作る（ポーリングはしない）。
- **アイコン I/O をUIスレッドでやらない**: path→`Image` のデコードはコストがある。コンポーネント内に `HashMap<String,Image>` キャッシュを持ち、未キャッシュ時のみ読む。大きい画像はバックグラウンドでデコードして送る余地あり（当面は同期でも可）。
- **エラー耐性**: `run` は D-Bus 切断で落ちうる。`init` のスレッド側で `run` を loop + backoff で再起動（`mpris.rs` が1秒 sleep で粘るのと同じ発想）。
- **重複描画コスト**: KDE イベントは頻度が低いので、毎イベント `RequestRedraw` で問題なし（mpris と同様）。
```
