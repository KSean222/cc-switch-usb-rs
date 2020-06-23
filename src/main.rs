use std::time::Duration;


fn main() {
    loop {
        match SwitchConnection::try_connect() {
            Ok(mut conn) => {
                println!("Successfully connected to the switch!");
                let data = b"getMainNsoBase";
                conn.write_all(&(data.len() as u32 + 2).to_le_bytes())
                    .unwrap();
                conn.write_all(data).unwrap();
                std::thread::sleep(Duration::from_secs(1));
                let mut len = [0u8; 4];
                conn.read_all(&mut len).unwrap();
                let len = u32::from_le_bytes(len) as usize;
                let mut data = vec![0; len];
                conn.read_all(&mut data).unwrap();
                println!("Data length: {}", len);
                println!("Received: {:?}", data);
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                println!("Retrying in 5 seconds...");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

#[derive(Debug)]
enum SwitchConnectionError {
    SwitchNotFound,
    NoInterface,
    NoInterfaceDescriptor,
    NoInEndpoint,
    NoOutEndpoint,
    RusbError(rusb::Error),
}

impl From<rusb::Error> for SwitchConnectionError {
    fn from(err: rusb::Error) -> SwitchConnectionError {
        SwitchConnectionError::RusbError(err)
    }
}

struct SwitchConnection {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    endpoint_in: u8,
    endpoint_out: u8,
}

impl SwitchConnection {
    pub const SWITCH_VENDOR_ID: u16 = 0x057E;
    pub const SWITCH_PRODUCT_ID: u16 = 0x3000;
    pub fn try_connect() -> Result<SwitchConnection, SwitchConnectionError> {
        for device in rusb::devices()?.iter() {
            let device_desc = device.device_descriptor()?;
            if device_desc.vendor_id() == SwitchConnection::SWITCH_VENDOR_ID
                && device_desc.product_id() == SwitchConnection::SWITCH_PRODUCT_ID
            {
                let mut handle = device.open()?;
                handle.set_active_configuration(1)?;
                if let Some(interface) = device.active_config_descriptor()?.interfaces().next() {
                    if let Some(interface_desc) = interface.descriptors().next() {
                        let mut endpoint_in = None;
                        let mut endpoint_out = None;
                        for endpoint_desc in interface_desc.endpoint_descriptors() {
                            if endpoint_desc.transfer_type() == rusb::TransferType::Bulk {
                                match endpoint_desc.direction() {
                                    rusb::Direction::In => {
                                        if endpoint_in.is_none() {
                                            endpoint_in = Some(endpoint_desc.address())
                                        }
                                    }
                                    rusb::Direction::Out => {
                                        if endpoint_out.is_none() {
                                            endpoint_out = Some(endpoint_desc.address())
                                        }
                                    }
                                }
                            }
                            if endpoint_in.is_some() && endpoint_out.is_some() {
                                handle.claim_interface(interface.number())?;
                                return Ok(SwitchConnection {
                                    handle,
                                    endpoint_in: endpoint_in.unwrap(),
                                    endpoint_out: endpoint_out.unwrap(),
                                });
                            }
                        }
                        return Err(if endpoint_in.is_none() {
                            SwitchConnectionError::NoInEndpoint
                        } else {
                            SwitchConnectionError::NoOutEndpoint
                        });
                    } else {
                        return Err(SwitchConnectionError::NoInterfaceDescriptor);
                    }
                } else {
                    return Err(SwitchConnectionError::NoInterface);
                }
            }
        }
        Err(SwitchConnectionError::SwitchNotFound)
    }
    pub fn read(&mut self, buf: &mut [u8]) -> rusb::Result<usize> {
        self.handle
            .read_bulk(self.endpoint_in, buf, Duration::from_secs(0))
    }
    pub fn read_all(&mut self, buf: &mut [u8]) -> Result<usize, (usize, rusb::Error)> {
        let mut read: usize = 0;
        while read < buf.len() {
            read += self.read(&mut buf[read..]).map_err(|e| (read, e))?;
        }
        Ok(read)
    }
    pub fn write(&mut self, buf: &[u8]) -> rusb::Result<usize> {
        self.handle
            .write_bulk(self.endpoint_out, buf, Duration::from_secs(0))
    }
    pub fn write_all(&mut self, buf: &[u8]) -> Result<usize, (usize, rusb::Error)> {
        let mut written: usize = 0;
        while written < buf.len() {
            written += self.write(&buf[written..]).map_err(|e| (written, e))?;
        }
        Ok(written)
    }
}
