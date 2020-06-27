use libtetris::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", content = "args")]
enum Command {
    Launch {
        options: cold_clear::Options,
        evaluator: cold_clear::evaluation::Standard,
    },
    Drop {
        handle: u32,
    },
    RequestNextMove {
        handle: u32,
        incoming: u32,
    },
    PollNextMove {
        handle: u32,
    },
    BlockNextMove {
        handle: u32,
    },
    AddNextPiece {
        handle: u32,
        piece: libtetris::Piece,
    },
    DefaultOptions,
    DefaultEvaluator,
}

fn main() {
    fn command(conn: &mut SwitchConnection) -> Command {
        let mut len = [0; 4];
        conn.read_all(&mut len).unwrap();
        let len = u32::from_le_bytes(len) as usize;
        let mut buf = vec![0; len];
        conn.read_all(&mut buf).unwrap();
        serde_cbor::from_slice(&buf).unwrap()
    }
    fn result(conn: &mut SwitchConnection, msg: &impl Serialize) {
        let buf = serde_cbor::to_vec(msg).unwrap();
        conn.write_all(&(buf.len() as u32).to_be_bytes()).unwrap();
        conn.write_all(&buf).unwrap();
    }
    loop {
        match SwitchConnection::try_connect() {
            Ok(mut conn) => {
                println!("Successfully connected to the switch!");
                let mut handle_counter: u32 = 0;
                let mut handles = HashMap::new();
                loop {
                    match command(&mut conn) {
                        Command::Launch { options, evaluator } => {
                            let interface =
                                cold_clear::Interface::launch(Board::new(), options, evaluator);
                            handle_counter = handle_counter.wrapping_add(1);
                            handles.insert(handle_counter, interface);
                            result(&mut conn, &handle_counter);
                        }
                        Command::Drop { handle } => {
                            handles.remove(&handle);
                        }
                        Command::RequestNextMove { handle, incoming } => {
                            handles.get(&handle).unwrap().request_next_move(incoming);
                        }
                        Command::PollNextMove { handle } => {
                            result(&mut conn, &handles.get(&handle).unwrap().poll_next_move());
                        }
                        Command::BlockNextMove { handle } => {
                            result(&mut conn, &handles.get(&handle).unwrap().block_next_move());
                        }
                        Command::AddNextPiece { handle, piece } => {
                            handles.get(&handle).unwrap().add_next_piece(piece);
                        }
                        Command::DefaultOptions => {
                            result(&mut conn, &cold_clear::Options::default());
                        }
                        Command::DefaultEvaluator => {
                            result(&mut conn, &cold_clear::evaluation::Standard::default());
                        }
                    }
                }
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
            match self.read(&mut buf[read..]) {
                Ok(bytes) => read += bytes,
                Err(rusb::Error::Timeout) => {}
                Err(err) => return Err((read, err)),
            }
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
            match self.write(&buf[written..]) {
                Ok(bytes) => written += bytes,
                Err(rusb::Error::Timeout) => {}
                Err(err) => return Err((written, err)),
            }
        }
        Ok(written)
    }
}
