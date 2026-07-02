use calloop::channel::Sender;

mod proxy;
pub use proxy::*;

#[derive(Clone)]
pub enum KDEConnectEvent {
    DeviceConnected(String, String),              //device name, ID
    DeviceDisconnected(String, String, String),   //device name, ID, Reason
    FileTransferRequest(String),                  //idk params
    FileTransferRequestReject(String),            //idk to
    SMSMessage(String, String, String),           //number, name, detail
    PhoneCall(String, String),                    //number, name
    NotificationReceived(String, String, String), //(title, detail, icon)?
    NotificationRemoved(String, String, String),  //(title, detail, icon)?
}

pub enum KDEConnectCommand {}

pub struct KdeConnectClient<'p> {
    connection: zbus::Connection,
    daemon: DaemonProxy<'p>,
    sender: Sender<KDEConnectEvent>,
    //device
    //
}

impl<'p> KdeConnectClient<'p> {
    pub fn init(sender: Sender<KDEConnectEvent>) {}
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
