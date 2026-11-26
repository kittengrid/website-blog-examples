use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use smoltcp::wire::{HardwareAddress, IpAddress, IpCidr};

struct MySimpleDevice {
    rx: Option<Vec<u8>>,
}

impl MySimpleDevice {
    fn new() -> Self {
        Self { rx: None }
    }

    fn send_packet(&mut self, packet: &[u8]) {
        self.rx = Some(packet.to_vec());
    }
}

struct MySimpleRxToken(Vec<u8>);
impl phy::RxToken for MySimpleRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        println!("RX: {:?}", self.0);
        f(&self.0)
    }
}

struct MySimpleTxToken;
impl phy::TxToken for MySimpleTxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = vec![0; len];
        let result = f(&mut buffer);

        println!("TX: {:?}", buffer);
        result
    }
}

impl phy::Device for MySimpleDevice {
    type RxToken<'a>
        = MySimpleRxToken
    where
        Self: 'a;
    type TxToken<'a>
        = MySimpleTxToken
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        self.rx
            .take()
            .map(|packet| (MySimpleRxToken(packet), MySimpleTxToken))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(MySimpleTxToken)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = 1500;
        caps
    }
}

fn main() {
    let mut device = MySimpleDevice::new();

    let config = Config::new(HardwareAddress::Ip);
    let mut iface = Interface::new(config, &mut device, Instant::now());

    iface.update_ip_addrs(|addrs| {
        addrs
            .push(IpCidr::new(IpAddress::v4(10, 166, 0, 2), 24))
            .unwrap();
    });

    // IPv4 ICMP Echo Request: 10.166.0.1 -> 10.166.0.2
    device.send_packet(&[
        0x45, 0x00, 0x00, 0x1c, 0x00, 0x01, 0x00, 0x00, 0x40, 0x01, 0x65, 0x92, 0x0a, 0xa6, 0x00,
        0x01, 0x0a, 0xa6, 0x00, 0x02, 0x08, 0x00, 0xe5, 0xca, 0x12, 0x34, 0x00, 0x01,
    ]);

    let mut sockets = SocketSet::new(vec![]);
    iface.poll(Instant::now(), &mut device, &mut sockets);
}
