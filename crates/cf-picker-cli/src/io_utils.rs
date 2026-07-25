use std::{
    fs::File,
    io::{BufRead, BufReader, Error},
    path::Path,
};

use colored::Colorize;

use cf_picker_core::{
    LineReader, Rand, models::CloudflareIpData, parse_ip_ranges, parse_ip_ranges_from_str,
};
use csv::WriterBuilder;

pub struct RandImpl;

impl Rand for RandImpl {
    fn next_u64(&mut self) -> u64 {
        rand::random::<u64>()
    }

    fn random_range_usize(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        let range = max as u128 - min as u128 + 1;
        (min as u128 + (rand::random::<u64>() as u128 % range)) as usize
    }

    fn random_range_u8(&mut self, min: u8, max: u8) -> u8 {
        if min >= max {
            return min;
        }
        let range = max as u64 - min as u64 + 1;
        min + (rand::random::<u64>() % range) as u8
    }
}

pub struct BufLineReader<R>(pub R);

impl<R: BufRead> LineReader for BufLineReader<R> {
    type Error = Error;

    fn read_line(&mut self, buf: &mut String) -> Result<usize, Self::Error> {
        self.0.read_line(buf)
    }
}

pub fn load_ip_ranges(
    ip_text: &str,
    ip_file: &str,
    test_all: bool,
) -> Result<Vec<std::net::IpAddr>, Error> {
    let mut rng = RandImpl;
    let warn_cb = |line: &str| {
        eprintln!(
            "{}",
            format!("[警告] 无效的 IP/CIDR 行，已跳过: {line}").yellow()
        );
    };

    if !ip_text.trim().is_empty() {
        Ok(parse_ip_ranges_from_str(
            ip_text,
            test_all,
            &mut rng,
            Some(warn_cb),
        ))
    } else {
        let file_path = if ip_file.is_empty() {
            "ip.txt"
        } else {
            ip_file
        };
        let file = File::open(file_path)?;
        let reader = BufReader::new(file);
        parse_ip_ranges(BufLineReader(reader), test_all, &mut rng, Some(warn_cb))
    }
}

pub fn export_csv<P: AsRef<Path>>(path: P, data: &[CloudflareIpData]) -> Result<(), Error> {
    if data.is_empty() {
        return Ok(());
    }

    let file = File::create(path)?;
    let mut wtr = WriterBuilder::new().from_writer(file);

    wtr.write_record([
        "IP 地址",
        "已发送",
        "已接收",
        "丢包率",
        "平均延迟",
        "下载速度(MB/s)",
        "地区码",
    ])?;

    for item in data {
        wtr.write_record(item.to_record())?;
    }

    wtr.flush()?;
    Ok(())
}
