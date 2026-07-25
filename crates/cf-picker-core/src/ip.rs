use core::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};

use crate::traits::{LineReader, Rand};

pub fn parse_ip_ranges<R: LineReader, G: Rand, W: FnMut(&str)>(
    mut reader: R,
    test_all: bool,
    rng: &mut G,
    mut on_warning: Option<W>,
) -> Result<Vec<IpAddr>, R::Error> {
    let mut ips = Vec::new();
    let mut buf = String::new();
    loop {
        buf.clear();
        let bytes_read = reader.read_line(&mut buf)?;
        if bytes_read == 0 {
            break;
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        parse_and_append_cidr(trimmed, test_all, &mut ips, rng, on_warning.as_mut());
    }
    Ok(ips)
}

pub fn parse_ip_ranges_from_str<G: Rand, W: FnMut(&str)>(
    ip_text: &str,
    test_all: bool,
    rng: &mut G,
    mut on_warning: Option<W>,
) -> Vec<IpAddr> {
    let mut ips = Vec::new();
    for line in ip_text.lines() {
        for segment in line.split(',') {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                continue;
            }
            parse_and_append_cidr(trimmed, test_all, &mut ips, rng, on_warning.as_mut());
        }
    }
    ips
}

fn parse_and_append_cidr<G: Rand, W: FnMut(&str)>(
    text: &str,
    test_all: bool,
    out: &mut Vec<IpAddr>,
    rng: &mut G,
    mut on_warning: Option<&mut W>,
) {
    let text_with_mask = if text.contains('/') {
        text.to_string()
    } else if text.contains('.') {
        format!("{text}/32")
    } else {
        format!("{text}/128")
    };

    let Ok(network) = IpNetwork::from_str(&text_with_mask) else {
        if let Some(ref mut cb) = on_warning {
            cb(text);
        }
        return;
    };

    match network {
        IpNetwork::V4(net) => choose_ipv4(net, test_all, out, rng),
        IpNetwork::V6(net) => choose_ipv6(net, out, rng),
    }
}

fn choose_ipv4<G: Rand>(net: Ipv4Network, test_all: bool, out: &mut Vec<IpAddr>, rng: &mut G) {
    if net.prefix() == 32 {
        out.push(IpAddr::V4(net.ip()));
        return;
    }

    let prefix = net.prefix();

    if prefix >= 24 {
        let first_octets = net.ip().octets();
        let mask_last = net.mask().octets()[3];
        let min_last = first_octets[3] & mask_last;
        let hosts = (!mask_last) as usize;

        if test_all {
            for i in 0..=hosts {
                let d = min_last.wrapping_add(i as u8);
                out.push(IpAddr::V4(Ipv4Addr::new(
                    first_octets[0],
                    first_octets[1],
                    first_octets[2],
                    d,
                )));
            }
        } else {
            let rand_offset = rng.random_range_usize(0, hosts) as u8;
            out.push(IpAddr::V4(Ipv4Addr::new(
                first_octets[0],
                first_octets[1],
                first_octets[2],
                min_last.wrapping_add(rand_offset),
            )));
        }
    } else {
        let start_ip = u32::from(net.ip());
        let end_ip = u32::from(net.broadcast());
        let mut curr = start_ip;

        while curr <= end_ip {
            let octets = Ipv4Addr::from(curr).octets();
            if test_all {
                for d in 0..=255u16 {
                    out.push(IpAddr::V4(Ipv4Addr::new(
                        octets[0], octets[1], octets[2], d as u8,
                    )));
                }
            } else {
                let rand_last = rng.random_range_u8(0, 255);
                out.push(IpAddr::V4(Ipv4Addr::new(
                    octets[0], octets[1], octets[2], rand_last,
                )));
            }
            let next_curr = curr.saturating_add(256);
            if next_curr <= curr {
                break;
            }
            curr = next_curr;
        }
    }
}

fn choose_ipv6<G: Rand>(net: Ipv6Network, out: &mut Vec<IpAddr>, rng: &mut G) {
    if net.prefix() == 128 {
        out.push(IpAddr::V6(net.ip()));
        return;
    }

    let mut octets = net.ip().octets();

    octets[14] = rng.random_range_u8(0, 255);
    octets[15] = rng.random_range_u8(0, 255);
    out.push(IpAddr::V6(Ipv6Addr::from(octets)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    struct MockRand;
    impl Rand for MockRand {
        fn next_u64(&mut self) -> u64 {
            42
        }

        fn random_range_usize(&mut self, _min: usize, max: usize) -> usize {
            max / 2
        }

        fn random_range_u8(&mut self, _min: u8, max: u8) -> u8 {
            max / 2
        }
    }

    #[test]
    fn test_parse_single_ip() {
        let mut ips = Vec::new();
        let mut rng = MockRand;
        parse_and_append_cidr("1.1.1.1", false, &mut ips, &mut rng, None::<&mut fn(&str)>);
        assert_eq!(ips.len(), 1);
        assert_eq!(ips[0], "1.1.1.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_parse_cidr_ipv4() {
        let mut ips = Vec::new();
        let mut rng = MockRand;
        parse_and_append_cidr(
            "192.168.1.0/24",
            true,
            &mut ips,
            &mut rng,
            None::<&mut fn(&str)>,
        );
        assert_eq!(ips.len(), 256);
    }

    #[test]
    fn test_parse_invalid_cidr_warning() {
        let mut rng = MockRand;
        let mut warned = Vec::new();
        let ips = parse_ip_ranges_from_str(
            "1.1.1.1, invalid_ip_format, 1.0.0.1",
            false,
            &mut rng,
            Some(|line: &str| warned.push(line.to_string())),
        );
        assert_eq!(ips.len(), 2);
        assert_eq!(warned, vec!["invalid_ip_format".to_string()]);
    }
}
