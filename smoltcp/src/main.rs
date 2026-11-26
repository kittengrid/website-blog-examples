use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use smoltcp::wire::{HardwareAddress, IpAddress, IpCidr};

struct StmPhy {
    rx_buffer: [u8; 1536],
    tx_buffer: [u8; 1536],
    rx_len: Option<usize>,
}

impl<'a> StmPhy {
    fn new() -> StmPhy {
        StmPhy {
            rx_buffer: [0; 1536],
            tx_buffer: [0; 1536],
            rx_len: None,
        }
    }
}

impl phy::Device for StmPhy {
    type RxToken<'a>
        = StmPhyRxToken<'a>
    where
        Self: 'a;
    type TxToken<'a>
        = StmPhyTxToken<'a>
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        println!("receive called");
        let len = self.rx_len.take()?;
        Some((
            StmPhyRxToken(&mut self.rx_buffer[..len]),
            StmPhyTxToken(&mut self.tx_buffer[..]),
        ))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        println!("transmit called");
        Some(StmPhyTxToken(&mut self.tx_buffer[..]))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1536;
        caps.max_burst_size = Some(1);
        caps.medium = Medium::Ip;
        caps
    }
}

impl StmPhy {
    fn send_packet(&mut self, packet: &[u8]) {
        self.rx_buffer[..packet.len()].copy_from_slice(packet);
        self.rx_len = Some(packet.len());
    }
}

struct StmPhyRxToken<'a>(&'a mut [u8]);

impl<'a> phy::RxToken for StmPhyRxToken<'a> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        // TODO: receive packet into buffer
        let result = f(&self.0);
        println!("rx called");
        println!("rx buffer: {:?}", &self.0);
        result
    }
}

struct StmPhyTxToken<'a>(&'a mut [u8]);

impl<'a> phy::TxToken for StmPhyTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let result = f(&mut self.0[..len]);
        println!("tx called {}", len);
        println!("tx buffer: {:?}", &self.0[..len]);
        // TODO: send packet out
        result
    }
}
fn main() {
    let mut device = StmPhy::new();
    let config = Config::new(HardwareAddress::Ip);
    let mut iface = Interface::new(config, &mut device, Instant::now());
    iface.update_ip_addrs(|addrs| {
        addrs
            .push(IpCidr::new(IpAddress::v4(10, 166, 0, 2), 24))
            .unwrap();
    });

    device.send_packet(&[
        // IPv4: 10.166.0.1 -> 10.166.0.2
        0x45, 0x00, 0x00, 0x1c, 0x00, 0x01, 0x00, 0x00, 0x40, 0x01, 0x65, 0x92, 0x0a, 0xa6, 0x00,
        0x01, 0x0a, 0xa6, 0x00, 0x02, // ICMP Echo Request
        0x08, 0x00, 0xe5, 0xca, 0x12, 0x34, 0x00, 0x01,
    ]);

    let timestamp = Instant::now();
    let mut sockets = SocketSet::new(vec![]);
    iface.poll(timestamp, &mut device, &mut sockets);
}
