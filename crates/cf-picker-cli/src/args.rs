use std::path::Path;
use std::time::Duration;

use clap::{CommandFactory, FromArgMatches, Parser};
use figment::{
    Figment,
    providers::{Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

use cf_picker_core::models::PickerOptions;

#[derive(Parser, Debug, Serialize, Deserialize, Clone)]
#[command(
    name = "CF-Picker",
    author = "shaogme",
    version = crate::VERSION,
    disable_version_flag = true,
    about = "测试各个 CDN 或网站所有 IP 的延迟和速度，获取最快 IP (IPv4+IPv6)！\nhttps://github.com/shaogme/cf-picker",
    help_template = "{about-section}\n\n参数：\n{options}"
)]
pub struct CliArgs {
    /// 配置文件路径；默认加载 config.toml (如果存在)
    #[arg(short = 'c', long = "config")]
    pub config: Option<String>,

    /// 延迟测速线程 (默认 200 最多 1000)
    #[arg(short = 'n', default_value_t = 200)]
    pub routines: usize,

    /// 延迟测速次数；单个 IP 延迟测速的次数 (默认 4 次)
    #[arg(short = 't', default_value_t = 4)]
    pub ping_times: usize,

    /// 下载测速数量；延迟测速并排序后，从最低延迟起下载测速的数量 (默认 10 个)
    #[arg(long = "dn", default_value_t = 10)]
    pub test_count: usize,

    /// 下载测速时间；单个 IP 下载测速最长时间 (默认 10 秒)
    #[arg(long = "dt", default_value_t = 10)]
    pub download_time: u64,

    /// 指定测速端口；延迟测速/下载测速时使用的端口 (默认 443 端口)
    #[arg(long = "tp", default_value_t = 443)]
    pub tcp_port: u16,

    /// 指定测速地址 (默认 https://speed.cloudflare.com/__down?bytes=500000000)
    #[arg(
        long = "url",
        default_value = "https://speed.cloudflare.com/__down?bytes=500000000"
    )]
    pub url: String,

    /// 切换测速模式；延迟测速模式改为 HTTP 协议 (默认 TCPing)
    #[arg(long = "httping", default_value_t = false)]
    pub httping: bool,

    /// 有效状态代码；HTTPing 延迟测速时网页返回的有效 HTTP 状态码 (默认 200 301 302)
    #[arg(long = "httping-code", default_value_t = 0)]
    pub httping_status_code: u16,

    /// 匹配指定地区；IATA 机场地区码或国家/城市码，英文逗号分隔 (默认 所有地区)
    #[arg(long = "cfcolo", default_value = "")]
    pub cf_colo: String,

    /// 平均延迟上限；只输出低于指定平均延迟的 IP (默认 9999 ms)
    #[arg(long = "tl", default_value_t = 9999)]
    pub max_delay: u64,

    /// 平均延迟下限；只输出高于指定平均延迟的 IP (默认 0 ms)
    #[arg(long = "tll", default_value_t = 0)]
    pub min_delay: u64,

    /// 丢包几率上限；只输出低于/等于指定丢包率的 IP (默认 1.00)
    #[arg(long = "tlr", default_value_t = 1.0)]
    pub max_loss_rate: f32,

    /// 下载速度下限；只输出高于指定下载速度的 IP (MB/s) (默认 0.00 MB/s)
    #[arg(long = "sl", default_value_t = 0.0)]
    pub min_speed: f64,

    /// 显示结果数量；测速后直接显示指定数量的结果 (默认 10 个)
    #[arg(short = 'p', default_value_t = 10)]
    pub print_num: usize,

    /// IP段数据文件 (默认 ip.txt)
    #[arg(short = 'f', default_value = "ip.txt")]
    pub ip_file: String,

    /// 指定IP段数据；英文逗号分隔 (默认 空)
    #[arg(long = "ip", default_value = "")]
    pub ip_text: String,

    /// 写入结果文件 (默认 result.csv)
    #[arg(short = 'o', default_value = "result.csv")]
    pub output: String,

    /// 禁用下载测速 (默认 启用)
    #[arg(long = "dd", default_value_t = false)]
    pub disable_download: bool,

    /// TCP 延迟测速超时时间，单位毫秒 (默认 1000 ms)
    #[arg(long = "tcping-timeout", default_value_t = 1000)]
    pub tcping_timeout: u64,

    /// HTTP 延迟测速超时时间，单位毫秒 (默认 2000 ms)
    #[arg(long = "httping-timeout", default_value_t = 2000)]
    pub httping_timeout: u64,

    /// 测速全部的IP；仅支持 IPv4 (默认 每个 /24 段随机测速一个 IP)
    #[arg(long = "allip", default_value_t = false)]
    pub test_all: bool,

    /// 调试输出模式 (默认 关闭)
    #[arg(long = "debug", default_value_t = false)]
    pub debug: bool,

    /// 打印程序版本
    #[arg(short = 'v', long = "version", default_value_t = false)]
    pub version: bool,
}

impl CliArgs {
    /// 结合配置文件（TOML）与命令行参数进行合并加载。
    /// 优先级：命令行显式覆盖 > 配置文件 (config.toml 或指定的 -c 参数) > 命令行默认值
    pub fn load() -> Self {
        let cli_matches = Self::command().get_matches();
        let cli_args = Self::from_arg_matches(&cli_matches).unwrap_or_else(|e| e.exit());

        // 决定 TOML 配置文件路径：如果命令行指定了 -c/--config 使用指定的；否则尝试默认 config.toml
        let config_path = cli_args
            .config
            .clone()
            .unwrap_or_else(|| "config.toml".to_string());

        if !Path::new(&config_path).exists() {
            // 不存在配置文件且用户未显式指定配置路径，直接返回 CLI 解析的参数
            return cli_args;
        }

        // 构筑图层：
        // 1. 底层 (Defaults): CLI 解析得到的默认配置结构
        // 2. 中层 (Middle): TOML 配置文件 (覆盖 CLI 默认值)
        // 3. 顶层 (Globals): 仅包含命令行中显式指定的参数 (覆盖 TOML 配置文件)
        let mut override_figment =
            Figment::from(Serialized::defaults(cli_args.clone())).merge(Toml::file(&config_path));

        // 对每一个命令行中显式指定的参数，在顶层覆盖
        let mut cli_overrides = serde_json::Map::new();
        let value_ref = match serde_json::to_value(&cli_args) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        };

        for arg in Self::command().get_arguments() {
            let id = arg.get_id().as_str();
            if let Some(source) = cli_matches.value_source(id)
                && source == clap::parser::ValueSource::CommandLine
                && let Some(val) = value_ref.get(id)
            {
                cli_overrides.insert(id.to_string(), val.clone());
            }
        }

        if !cli_overrides.is_empty() {
            override_figment = override_figment.merge(Serialized::globals(
                serde_json::Value::Object(cli_overrides),
            ));
        }

        match override_figment.extract() {
            Ok(merged) => merged,
            Err(err) => {
                eprintln!(
                    "[警告] 加载配置文件 {} 失败: {err}，退回使用命令行参数",
                    config_path
                );
                cli_args
            }
        }
    }
}

impl From<&CliArgs> for PickerOptions {
    fn from(args: &CliArgs) -> Self {
        Self {
            routines: args.routines,
            ping_times: args.ping_times,
            test_count: args.test_count,
            download_time: Duration::from_secs(args.download_time),
            tcp_port: args.tcp_port,
            url: args.url.clone(),
            httping: args.httping,
            httping_status_code: args.httping_status_code,
            tcping_timeout: Duration::from_millis(args.tcping_timeout),
            httping_timeout: Duration::from_millis(args.httping_timeout),
            cf_colo: args.cf_colo.clone(),
            max_delay: Duration::from_millis(args.max_delay),
            min_delay: Duration::from_millis(args.min_delay),
            max_loss_rate: args.max_loss_rate,
            min_speed: args.min_speed * 1024.0 * 1024.0, // Convert MB/s to bytes/s
            ip_file: args.ip_file.clone(),
            ip_text: args.ip_text.clone(),
            disable_download: args.disable_download,
            test_all: args.test_all,
            debug: args.debug,
        }
    }
}
