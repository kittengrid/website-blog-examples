use etherparse::{Icmpv4Type, NetSlice, PacketBuilder, SlicedPacket, TransportSlice};
use libc::ioctl;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;

#[repr(C)]
struct IfReq {
    ifr_name: [u8; 16],
    ifr_flags: libc::c_short,
    ifr_ifru: [u8; 24],
}

/// Returns a raw ICMP Echo Request packet (ping) from `src` to `dst`.
fn ping_echo_request(src: [u8; 4], dst: [u8; 4]) -> Vec<u8> {
    let builder = PacketBuilder::ipv4(src, dst, 20).icmpv4_echo_request(1, 1);

    // payload of the echo request
    let payload = b"ola";

    let mut result = Vec::<u8>::with_capacity(builder.size(payload.len()));
    builder.write(&mut result, payload).unwrap();

    result
}

/// This is just returning a file descriptor to the TUN device.
/// What we did in bash was just declaring the interface and assigning it an IP, but
/// to obtain a file descriptor to the TUN device we need to use the ioctl syscall with the TUNSETIFF flag.
///
/// You can see an example of what I do in here https://docs.kernel.org/networking/tuntap.html
fn create_tun(name: &str) -> Result<File, Box<dyn std::error::Error>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/net/tun")?;

    let mut ifreq = IfReq {
        ifr_name: [0; 16],
        ifr_flags: (libc::IFF_TUN | libc::IFF_NO_PI) as libc::c_short,
        ifr_ifru: [0; 24],
    };

    // Copy the name of the TUN device into the ifreq struct
    for (i, c) in name.bytes().enumerate() {
        ifreq.ifr_name[i] = c;
    }

    unsafe {
        ioctl(file.as_raw_fd(), libc::TUNSETIFF, &ifreq);
    }

    Ok(file)
}

/**
 * This is a simple program that uses a TUN interface to send
 * an ICMP Echo Request directly to the local kernel and
 * receive its Echo Reply.
 *
 * First, create a TUN interface and assign it an IP address:
 *
 * sudo ip tuntap add dev tun0 mode tun
 * sudo ip addr add 10.166.0.2/24 dev tun0
 * sudo ip link set dev tun0 up
 *
 **/
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Obtain a file descriptor to read/write to the interface
    let mut tun = create_tun("tun0")?;

    // IPv4 ICMP Echo Request: 10.166.0.1 -> 10.166.0.2
    let echo_request = ping_echo_request([10, 166, 0, 1], [10, 166, 0, 2]);

    // Now we send the ping
    tun.write(&echo_request)?;

    // Now we wait for the echo reply
    // (discarding what we receive until we get the echo reply)
    let mut buffer = [0u8; 65535];

    loop {
        let n = tun.read(&mut buffer)?;
        let packet = SlicedPacket::from_ip(&buffer[..n])?;

        if let Some(TransportSlice::Icmpv4(icmp)) = packet.transport {
            if let Icmpv4Type::EchoReply(header) = icmp.icmp_type() {
                if let Some(NetSlice::Ipv4(ip)) = packet.net {
                    println!(
                        "PONG: {} -> {}",
                        ip.header().source_addr(),
                        ip.header().destination_addr()
                    );
                }

                println!("id={}, seq={}", header.id, header.seq);
                println!("Payload: {}", String::from_utf8_lossy(icmp.payload()));

                break;
            }
        }
    }

    Ok(())
}
