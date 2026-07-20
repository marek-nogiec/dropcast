use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::DynError;

#[derive(Clone, Debug)]
pub struct CastDevice {
    pub name: String,
    pub address: IpAddr,
    pub model: Option<String>,
}

pub fn discover(timeout: Duration) -> Result<Vec<CastDevice>, DynError> {
    let daemon = ServiceDaemon::new()?;
    let receiver = daemon.browse("_googlecast._tcp.local.")?;
    let deadline = Instant::now() + timeout;
    let mut devices = BTreeMap::new();

    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(service)) => {
                let address = service
                    .get_addresses_v4()
                    .into_iter()
                    .next()
                    .map(IpAddr::V4)
                    .or_else(|| {
                        service
                            .get_addresses()
                            .iter()
                            .map(|address| address.to_ip_addr())
                            .find(|address| !address.is_loopback() && !address.is_unspecified())
                    });
                let Some(address) = address else {
                    continue;
                };

                let name = service
                    .get_property_val_str("fn")
                    .unwrap_or_else(|| service.get_fullname().split('.').next().unwrap_or("Cast"))
                    .to_owned();
                let id = service
                    .get_property_val_str("id")
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{name}@{address}"));
                let model = service.get_property_val_str("md").map(str::to_owned);

                devices.insert(
                    id.clone(),
                    CastDevice {
                        name,
                        address,
                        model,
                    },
                );
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    let _ = daemon.stop_browse("_googlecast._tcp.local.");
    let _ = daemon.shutdown();

    let mut devices: Vec<_> = devices.into_values().collect();
    devices.sort_by_key(|device| device.name.to_lowercase());
    Ok(devices)
}
