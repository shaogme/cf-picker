# CF-Picker

> **致谢**：特别感谢 [CloudflareSpeedTest](https://github.com/XIU2/CloudflareSpeedTest) 项目。本项目是该项目的 Rust 重写版本。

测试各个 CDN 或网站所有 IP 的延迟和速度，获取最快 IP (IPv4 + IPv6)。

---

## 项目简介

CF-Picker 采用 Rust 语言重新设计与实现，旨在提供高性能、低资源消耗且易于扩展的 Cloudflare / CDN IP 优选测速工具。

### 架构设计

项目采用 Workspace 多 Crate 模块化设计：

- **cf-picker-core**: 核心逻辑库，包含 IP 段解析、延迟测速与下载测速调度算法、Colo 地区码提取与匹配、核心数据模型及 Trait 抽象（无原生 IO 依赖，方便移植）。
- **cf-picker-tokio**: 基于 Tokio 异步运行时与 reqwest 实现的底层网络客户端，提供高并发 TCPing 和 HTTPing 能力。
- **cf-picker-cli**: 命令行交互工具，整合配置加载、参数解析、进度条展示以及 CSV 导出等功能。

---

## 主要特性

- **高效并发测速**：基于 Tokio 异步并发调度，大幅提升海量 IP 的延迟测速效率。
- **双延迟测速模式**：
  - **TCPing 模式**（默认）：基于 TCP 三次握手测试节点响应延迟。
  - **HTTPing 模式**：通过 HTTP HEAD 请求测试延迟，并提取 Response Header 中的 Cloudflare 地区码（Colo）。
- **智能节点筛选**：支持通过 IATA 机场代码或国家/地区代码筛选特定的 Cloudflare 节点（如 HKG、SJC、NRT 等）。
- **滑动 EWMA 下载测速**：基于指数加权移动平均算法平滑计算下载速率，精确评估节点实际下载性能。
- **配置与参数灵活组合**：支持读取 `config.toml` 配置文件，并允许命令行参数动态覆盖配置。
- **全面支持 IPv4 / IPv6**：支持解析 CIDR IP 段，并针对 IPv4 和 IPv6 提供随机抽样或全量测速策略。

---

## 命令行参数说明

可以通过 `cf-picker-cli --help` 查看完整命令行帮助信息：

| 参数 | 长参数 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `-c` | `--config` | `config.toml` | 指定配置文件路径 |
| `-n` | `--routines` | `200` | 延迟测速并发线程数（推荐上限 1000） |
| `-t` | `--ping-times` | `4` | 单个 IP 延迟测速的测试次数 |
| | `--dn` | `10` | 延迟测速排序后，参与下载测速的 IP 数量 |
| | `--dt` | `10` | 单个 IP 下载测速的最长持续时间（单位：秒） |
| | `--tp` | `443` | 测速使用的目标 TCP 端口 |
| | `--url` | `https://speed.cloudflare.com/__down?bytes=500000000` | 下载测速使用的测试 URL |
| | `--httping` | `false` | 切换为 HTTPing 延迟测速模式 |
| | `--httping-code` | `0` | HTTPing 测速有效状态码（默认 0 表示允许 200, 301, 302） |
| | `--tcping-timeout` | `1000` | TCP 延迟测速超时时间（单位：ms） |
| | `--httping-timeout` | `2000` | HTTP 延迟测速超时时间（单位：ms） |
| | `--cfcolo` | `""` | 匹配指定地区码（如 `HKG,SJC`，逗号分隔） |
| | `--tl` | `9999` | 平均延迟上限，仅保留低于该延迟的 IP（单位：ms） |
| | `--tll` | `0` | 平均延迟下限，仅保留高于该延迟的 IP（单位：ms） |
| | `--tlr` | `1.0` | 丢包率上限，仅保留低于或等于该丢包率的 IP |
| | `--sl` | `0.0` | 下载速度下限，仅保留高于该速度的 IP（单位：MB/s） |
| `-p` | `--print-num` | `10` | 控制台最终显示的前 N 个优选结果数量 |
| `-f` | `--ip-file` | `ip.txt` | IP 段数据文件路径 |
| | `--ip` | `""` | 直接在命令行指定 IP 或 IP 段（逗号分隔） |
| `-o` | `--output` | `result.csv` | 导出测速结果的 CSV 文件路径 |
| | `--dd` | `false` | 禁用下载测速，仅进行延迟测试 |
| | `--allip` | `false` | 测速 IP 段内的全部 IP（仅限 IPv4；默认每个 /24 段随机抽样一个 IP） |
| | `--debug` | `false` | 开启调试输出模式 |
| `-v` | `--version` | `false` | 打印程序版本信息 |

---

## 安装与使用

### 方法一：下载预编译产物

前往 [Releases 页面](https://github.com/shaogme/cf-picker/releases) 下载适用于您系统和架构的预编译二进制文件：

- **Windows**: `cf-picker-x86_64-pc-windows-gnu.tar.gz` (x64) / `cf-picker-aarch64-pc-windows-gnullvm.tar.gz` (ARM64)
- **Linux**: `cf-picker-x86_64-unknown-linux-gnu.tar.gz` (x64) / `cf-picker-aarch64-unknown-linux-gnu.tar.gz` (ARM64)
- **macOS**: `cf-picker-x86_64-apple-darwin.tar.gz` (Intel) / `cf-picker-aarch64-apple-darwin.tar.gz` (Apple Silicon)

解压后即可直接运行可执行文件。

### 方法二：从源码构建

确保已安装 Rust 工具链（推荐 1.85+ / 2024 Edition 兼容）：

```bash
# 克隆仓库
git clone https://github.com/shaogme/cf-picker.git
cd cf-picker

# 编译 Release 版本
cargo build --release -p cf-picker-cli
```

编译生成的二进制文件位于 `target/release/cf-picker-cli`（Windows 下为 `cf-picker-cli.exe`）。

---

## 运行测速

1. 默认参数运行（读取当前目录下的 `ip.txt` 文件）：

```bash
./cf-picker-cli
```

2. 指定 IP 段并开启 HTTPing 匹配中国香港 (HKG) 节点：

```bash
./cf-picker-cli --ip "104.16.0.0/13" --httping --cfcolo HKG
```

3. 禁用下载测试，仅测试 TCP 延迟并打印前 20 个最优 IP：

```bash
./cf-picker-cli --dd -p 20
```

---

## 配置文件支持

项目支持使用 `config.toml` 管理常用测速配置。命令行参数将自动覆盖配置文件中的同名项：

```toml
routines = 200
ping_times = 4
test_count = 10
download_time = 10
tcp_port = 443
url = "https://speed.cloudflare.com/__down?bytes=500000000"
httping = false
cf_colo = ""
max_delay = 9999
min_delay = 0
max_loss_rate = 1.0
min_speed = 0.0
print_num = 10
ip_file = "ip.txt"
output = "result.csv"
disable_download = false
test_all = false
```

---

## 开源许可

本项目遵循 [GPL-3.0 许可协议](LICENSE)。

