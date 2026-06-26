use std::collections::HashMap;
use zbus::proxy;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_devices() {
        let conn = zbus::Connection::session().await.unwrap();
        let proxy = DaemonProxy::new(&conn).await.unwrap();
        let devices = proxy.device_names().await.unwrap();
        println!("devices ({}):", devices.len());
        for id in &devices {
            println!("{:?}", id)
        }
    }
}
