//! Real HID transport. Windows-only (the keyboard lives on the Windows host).

use crate::transport::{DeviceError, Transport};
use hidapi::{HidApi, HidDevice};
use std::time::Duration;

/// (VID, PID) pairs from the vendor bundle's WebHID filters.
pub const IDS: &[(u16, u16)] = &[(0x3879, 0x0806), (0x1CAA, 0x0806)];
pub const USAGE_PAGE: u16 = 0xFFA0;
pub const USAGE: u16 = 0x01;

pub struct HidTransport {
    dev: HidDevice,
}

impl HidTransport {
    pub fn open() -> Result<Self, DeviceError> {
        let api = HidApi::new().map_err(|e| DeviceError::Io(e.to_string()))?;
        let mut seen_device = false;
        for info in api.device_list() {
            let id = (info.vendor_id(), info.product_id());
            if !IDS.contains(&id) {
                continue;
            }
            seen_device = true;
            if info.usage_page() == USAGE_PAGE && info.usage() == USAGE {
                return match info.open_device(&api) {
                    Ok(dev) => Ok(Self { dev }),
                    Err(e) => Err(DeviceError::Busy(e.to_string())),
                };
            }
        }
        if seen_device {
            // Right VID/PID but the vendor collection didn't open, most likely held
            // exclusively by the web configurator.
            Err(DeviceError::Busy("vendor interface not available".into()))
        } else {
            Err(DeviceError::NotFound)
        }
    }
}

impl Transport for HidTransport {
    fn send(&mut self, report: &[u8; 64]) -> Result<(), DeviceError> {
        // Report ID 0 prepended per hidapi convention.
        let mut buf = [0u8; 65];
        buf[1..].copy_from_slice(report);
        self.dev.write(&buf).map_err(|e| DeviceError::Io(e.to_string()))?;
        Ok(())
    }
    fn recv(&mut self, timeout: Duration) -> Result<[u8; 64], DeviceError> {
        let mut buf = [0u8; 64];
        let n = self
            .dev
            .read_timeout(&mut buf, timeout.as_millis() as i32)
            .map_err(|e| DeviceError::Io(e.to_string()))?;
        if n == 0 {
            return Err(DeviceError::Timeout);
        }
        Ok(buf)
    }
}
