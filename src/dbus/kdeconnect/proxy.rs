use std::collections::HashMap;
use zbus::proxy;
use zbus::zvariant::OwnedValue;

#[proxy(
    interface = "org.kde.kdeconnect.daemon",
    default_service = "org.kde.kdeconnect",
    default_path = "/modules/kdeconnect"
)]
pub trait Daemon {
    // Properties
    #[zbus(property)]
    fn custom_devices(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn set_custom_devices(&self, devices: Vec<String>) -> zbus::Result<()>;
    #[zbus(property)]
    fn pairing_requests(&self) -> zbus::Result<Vec<String>>;

    // Methods
    fn announced_name(&self) -> zbus::Result<String>;
    fn device_id_by_name(&self, name: &str) -> zbus::Result<String>;

    #[zbus(name = "deviceNames")]
    fn device_names(&self) -> zbus::Result<HashMap<String, String>>;
    #[zbus(name = "deviceNames")]
    fn device_names_reachable(&self, only_reachable: bool)
        -> zbus::Result<HashMap<String, String>>;
    #[zbus(name = "deviceNames")]
    fn device_names_filtered(
        &self,
        only_reachable: bool,
        only_paired: bool,
    ) -> zbus::Result<HashMap<String, String>>;

    #[zbus(name = "devices")]
    fn devices(&self) -> zbus::Result<Vec<String>>;
    #[zbus(name = "devices")]
    fn devices_reachable(&self, only_reachable: bool) -> zbus::Result<Vec<String>>;
    #[zbus(name = "devices")]
    fn devices_filtered(
        &self,
        only_reachable: bool,
        only_paired: bool,
    ) -> zbus::Result<Vec<String>>;

    fn force_on_network_change(&self) -> zbus::Result<()>;
    fn link_providers(&self) -> zbus::Result<Vec<String>>;
    fn self_id(&self) -> zbus::Result<String>;
    fn send_simple_notification(
        &self,
        event_id: &str,
        title: &str,
        text: &str,
        icon_name: &str,
    ) -> zbus::Result<()>;
    fn set_announced_name(&self, name: &str) -> zbus::Result<()>;
    fn set_link_provider_state(&self, link_provider: &str, enabled: bool) -> zbus::Result<()>;

    // Signals
    #[zbus(signal)]
    fn announced_name_changed(&self, announced_name: String) -> zbus::Result<()>;
    #[zbus(signal, name = "customDevicesChanged")]
    fn on_custom_devices_changed(&self, custom_devices: Vec<String>) -> zbus::Result<()>;
    #[zbus(signal)]
    fn device_added(&self, id: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn device_list_changed(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn device_removed(&self, id: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn device_visibility_changed(&self, id: String, is_visible: bool) -> zbus::Result<()>;
    #[zbus(signal)]
    fn link_providers_changed(&self, link_providers: Vec<String>) -> zbus::Result<()>;
    #[zbus(signal, name = "pairingRequestsChanged")]
    fn on_pairing_requests_changed(&self) -> zbus::Result<()>;
}

// ==========================================================================
// Device — org.kde.kdeconnect.device
// path: /modules/kdeconnect/devices/<device-id>
// The paths below are per-device, so no `default_path` is set; the caller must
// supply the device object path when building the proxy.
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device",
    default_service = "org.kde.kdeconnect"
)]
pub trait Device {
    // Methods
    fn accept_pairing(&self) -> zbus::Result<()>;
    fn cancel_pairing(&self) -> zbus::Result<()>;
    fn encryption_info(&self) -> zbus::Result<String>;
    fn has_plugin(&self, name: &str) -> zbus::Result<bool>;
    fn is_pair_requested(&self) -> zbus::Result<bool>;
    fn is_pair_requested_by_peer(&self) -> zbus::Result<bool>;
    fn is_paired(&self) -> zbus::Result<bool>;
    fn is_plugin_enabled(&self, plugin_name: &str) -> zbus::Result<bool>;
    fn loaded_plugins(&self) -> zbus::Result<Vec<String>>;
    fn pair_state_as_int(&self) -> zbus::Result<i32>;
    fn plugin_icon_name(&self, plugin_name: &str) -> zbus::Result<String>;
    fn plugins_config_file(&self) -> zbus::Result<String>;
    fn reload_plugins(&self) -> zbus::Result<()>;
    fn request_pairing(&self) -> zbus::Result<()>;
    fn set_plugin_enabled(&self, plugin_name: &str, enabled: bool) -> zbus::Result<()>;
    fn unpair(&self) -> zbus::Result<()>;
    fn verification_key(&self) -> zbus::Result<String>;

    // Properties
    #[zbus(property)]
    fn active_provider_names(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;
    #[zbus(property, name = "isPairRequested")]
    fn is_pair_requested_prop(&self) -> zbus::Result<bool>;
    #[zbus(property, name = "isPairRequestedByPeer")]
    fn is_pair_requested_by_peer_prop(&self) -> zbus::Result<bool>;
    #[zbus(property, name = "isPaired")]
    fn is_paired_prop(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn is_reachable(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn pair_state(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn reachable_addresses(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn status_icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn supported_plugins(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property, name = "type")]
    fn device_type(&self) -> zbus::Result<String>;
    #[zbus(property, name = "verificationKey")]
    fn verification_key_prop(&self) -> zbus::Result<String>;

    // Signals
    #[zbus(signal)]
    fn links_changed(&self) -> zbus::Result<()>;
    #[zbus(signal, name = "nameChanged")]
    fn on_name_changed(&self, name: String) -> zbus::Result<()>;
    #[zbus(signal, name = "pairStateChanged")]
    fn on_pair_state_changed(&self, state: i32) -> zbus::Result<()>;
    #[zbus(signal)]
    fn pairing_failed(&self, error: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn plugins_changed(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn reachable_changed(&self, reachable: bool) -> zbus::Result<()>;
    #[zbus(signal, name = "statusIconNameChanged")]
    fn on_status_icon_name_changed(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn type_changed(&self, type_: String) -> zbus::Result<()>;
}

// ==========================================================================
// Conversations — org.kde.kdeconnect.device.conversations (device object path)
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.conversations",
    default_service = "org.kde.kdeconnect"
)]
pub trait Conversations {
    fn active_conversations(&self) -> zbus::Result<Vec<OwnedValue>>;
    fn reply_to_conversation(
        &self,
        conversation_id: i64,
        message: &str,
        attachment_urls: Vec<OwnedValue>,
    ) -> zbus::Result<()>;
    fn request_all_conversation_threads(&self) -> zbus::Result<()>;
    fn request_attachment_file(&self, part_id: i64, unique_identifier: &str) -> zbus::Result<()>;
    fn request_conversation(&self, conversation_id: i64, start: i32, end: i32) -> zbus::Result<()>;
    fn send_without_conversation(
        &self,
        address_list: Vec<OwnedValue>,
        message: &str,
        attachment_urls: Vec<OwnedValue>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn attachment_received(&self, part_id: String, unique_identifier: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn conversation_created(&self, conversation: OwnedValue) -> zbus::Result<()>;
    #[zbus(signal)]
    fn conversation_loaded(&self, conversation_id: i64, count: u64) -> zbus::Result<()>;
    #[zbus(signal)]
    fn conversation_removed(&self, conversation_id: i64) -> zbus::Result<()>;
    #[zbus(signal)]
    fn conversation_updated(&self, conversation: OwnedValue) -> zbus::Result<()>;
}

// ==========================================================================
// Battery — org.kde.kdeconnect.device.battery
// path: <device>/battery
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.battery",
    default_service = "org.kde.kdeconnect"
)]
pub trait Battery {
    #[zbus(property)]
    fn charge(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn is_charging(&self) -> zbus::Result<bool>;

    #[zbus(signal)]
    fn refreshed(&self, is_charging: bool, charge: i32) -> zbus::Result<()>;
}

// ==========================================================================
// Connectivity report — org.kde.kdeconnect.device.connectivity_report
// path: <device>/connectivity_report
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.connectivity_report",
    default_service = "org.kde.kdeconnect"
)]
pub trait ConnectivityReport {
    #[zbus(property)]
    fn cellular_network_strength(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn cellular_network_type(&self) -> zbus::Result<String>;

    #[zbus(signal, name = "refreshed")]
    fn connectivity_refreshed(&self, network_type: String, strength: i32) -> zbus::Result<()>;
}

// ==========================================================================
// Clipboard — org.kde.kdeconnect.device.clipboard
// path: <device>/clipboard
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.clipboard",
    default_service = "org.kde.kdeconnect"
)]
pub trait Clipboard {
    fn send_clipboard(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn is_auto_share_disabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_is_auto_share_disabled(&self, disabled: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    fn auto_share_disabled_changed(&self, disabled: bool) -> zbus::Result<()>;
}

// ==========================================================================
// Find my phone — org.kde.kdeconnect.device.findmyphone
// path: <device>/findmyphone
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.findmyphone",
    default_service = "org.kde.kdeconnect"
)]
pub trait FindMyPhone {
    fn ring(&self) -> zbus::Result<()>;
}

// ==========================================================================
// Ping — org.kde.kdeconnect.device.ping
// path: <device>/ping
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.ping",
    default_service = "org.kde.kdeconnect"
)]
pub trait Ping {
    #[zbus(name = "sendPing")]
    fn send_ping(&self) -> zbus::Result<()>;
    #[zbus(name = "sendPing")]
    fn send_ping_with_message(&self, custom_message: &str) -> zbus::Result<()>;
}

// ==========================================================================
// Share — org.kde.kdeconnect.device.share
// path: <device>/share
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.share",
    default_service = "org.kde.kdeconnect"
)]
pub trait Share {
    fn open_file(&self, url: &str) -> zbus::Result<()>;
    fn share_text(&self, text: &str) -> zbus::Result<()>;
    fn share_url(&self, url: &str) -> zbus::Result<()>;
    fn share_urls(&self, urls: Vec<String>) -> zbus::Result<()>;

    #[zbus(signal)]
    fn share_received(&self, url: String) -> zbus::Result<()>;
}

// ==========================================================================
// Notifications — org.kde.kdeconnect.device.notifications
// path: <device>/notifications
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.notifications",
    default_service = "org.kde.kdeconnect"
)]
pub trait Notifications {
    fn active_notifications(&self) -> zbus::Result<Vec<String>>;
    fn send_action(&self, key: &str, action: &str) -> zbus::Result<()>;
    fn send_reply(&self, reply_id: &str, message: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    fn all_notifications_removed(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn notification_posted(&self, public_id: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn notification_removed(&self, public_id: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn notification_updated(&self, public_id: String) -> zbus::Result<()>;
}

// ==========================================================================
// Notification — org.kde.kdeconnect.device.notifications.notification
// path: <device>/notifications/<id>
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.notifications.notification",
    default_service = "org.kde.kdeconnect"
)]
pub trait Notification {
    fn dismiss(&self) -> zbus::Result<()>;
    fn reply(&self) -> zbus::Result<()>;
    fn send_reply(&self, message: &str) -> zbus::Result<()>;

    #[zbus(property)]
    fn app_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn dismissable(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn group_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn has_icon(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn icon_path(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn internal_id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn is_conversation(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn is_group_conversation(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn reply_id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn silent(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn text(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn ticker(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn title(&self) -> zbus::Result<String>;

    #[zbus(signal)]
    fn ready(&self) -> zbus::Result<()>;
}

// ==========================================================================
// SMS — org.kde.kdeconnect.device.sms
// path: <device>/sms
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.sms",
    default_service = "org.kde.kdeconnect"
)]
pub trait Sms {
    fn get_attachment(&self, part_id: i64, unique_identifier: &str) -> zbus::Result<()>;
    fn launch_app(&self) -> zbus::Result<()>;
    fn request_all_conversations(&self) -> zbus::Result<()>;
    fn request_attachment(&self, part_id: i64, unique_identifier: &str) -> zbus::Result<()>;

    #[zbus(name = "requestConversation")]
    fn request_conversation(&self, conversation_id: i64) -> zbus::Result<()>;
    #[zbus(name = "requestConversation")]
    fn request_conversation_from(
        &self,
        conversation_id: i64,
        range_start_timestamp: i64,
    ) -> zbus::Result<()>;
    #[zbus(name = "requestConversation")]
    fn request_conversation_range(
        &self,
        conversation_id: i64,
        range_start_timestamp: i64,
        number_to_request: i64,
    ) -> zbus::Result<()>;

    #[zbus(name = "sendSms")]
    fn send_sms(
        &self,
        address_list: Vec<OwnedValue>,
        message_body: &str,
        attachment_urls: Vec<OwnedValue>,
    ) -> zbus::Result<()>;
    #[zbus(name = "sendSms")]
    fn send_sms_with_sub(
        &self,
        address_list: Vec<OwnedValue>,
        message_body: &str,
        attachment_urls: Vec<OwnedValue>,
        sub_id: i64,
    ) -> zbus::Result<()>;
}

// ==========================================================================
// Telephony — org.kde.kdeconnect.device.telephony
// path: <device>/telephony
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.telephony",
    default_service = "org.kde.kdeconnect"
)]
pub trait Telephony {
    #[zbus(signal)]
    fn call_received(
        &self,
        event: String,
        phone_number: String,
        contact_name: String,
    ) -> zbus::Result<()>;
}

// ==========================================================================
// MPRIS remote — org.kde.kdeconnect.device.mprisremote
// path: <device>/mprisremote
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.mprisremote",
    default_service = "org.kde.kdeconnect"
)]
pub trait MprisRemote {
    fn request_player_list(&self) -> zbus::Result<()>;
    fn seek(&self, offset: i32) -> zbus::Result<()>;
    fn send_action(&self, action: &str) -> zbus::Result<()>;

    #[zbus(property)]
    fn album(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn artist(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn can_seek(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn is_playing(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn length(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn local_album_art_url(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn player(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_player(&self, player: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn player_list(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn set_position(&self, position: i32) -> zbus::Result<()>;
    #[zbus(property)]
    fn title(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn volume(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn set_volume(&self, volume: i32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn properties_changed(&self) -> zbus::Result<()>;
}

// ==========================================================================
// Remote keyboard — org.kde.kdeconnect.device.remotekeyboard
// path: <device>/remotekeyboard
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.remotekeyboard",
    default_service = "org.kde.kdeconnect"
)]
pub trait RemoteKeyboard {
    #[zbus(name = "sendKeyPress")]
    fn send_key_press(
        &self,
        key: &str,
        special_key: i32,
        shift: bool,
        ctrl: bool,
        alt: bool,
    ) -> zbus::Result<()>;
    fn send_q_key_event(
        &self,
        event: HashMap<String, OwnedValue>,
        sending_only_modifiers: bool,
    ) -> zbus::Result<()>;
    fn translate_qt_key(&self, qt_key: i32) -> zbus::Result<i32>;

    #[zbus(property)]
    fn remote_state(&self) -> zbus::Result<bool>;

    #[zbus(signal)]
    fn key_press_received(
        &self,
        key: String,
        special_key: i32,
        shift: bool,
        ctrl: bool,
        alt: bool,
    ) -> zbus::Result<()>;
    #[zbus(signal, name = "remoteStateChanged")]
    fn on_remote_state_changed(&self, state: bool) -> zbus::Result<()>;
}

// ==========================================================================
// Remote control — org.kde.kdeconnect.device.remotecontrol
// path: <device>/remotecontrol
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.remotecontrol",
    default_service = "org.kde.kdeconnect"
)]
pub trait RemoteControl {
    fn move_cursor(&self, offset: (i32, i32)) -> zbus::Result<()>;
    fn send_command(&self, args: HashMap<String, OwnedValue>) -> zbus::Result<()>;
}

// ==========================================================================
// SFTP — org.kde.kdeconnect.device.sftp
// path: <device>/sftp
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.sftp",
    default_service = "org.kde.kdeconnect"
)]
pub trait Sftp {
    fn get_directories(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn get_mount_error(&self) -> zbus::Result<String>;
    fn is_mounted(&self) -> zbus::Result<bool>;
    fn mount(&self) -> zbus::Result<()>;
    fn mount_and_wait(&self) -> zbus::Result<bool>;
    fn mount_point(&self) -> zbus::Result<String>;
    fn start_browsing(&self) -> zbus::Result<bool>;
    fn unmount(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn mounted(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn unmounted(&self) -> zbus::Result<()>;
}

// ==========================================================================
// Contacts — org.kde.kdeconnect.device.contacts
// path: <device>/contacts
// ==========================================================================
#[proxy(
    interface = "org.kde.kdeconnect.device.contacts",
    default_service = "org.kde.kdeconnect"
)]
pub trait Contacts {
    fn synchronize_remote_with_local(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn local_cache_synchronized(&self, uids: Vec<String>) -> zbus::Result<()>;
}
